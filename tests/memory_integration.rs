//! Integration tests for the memory system.
//!
//! Tests the dual-layer memory architecture (file memory + structured SQLite memory),
//! covering lifecycle operations, cross-chat isolation, persistence across restarts,
//! and the memory quality gate.

use microclaw::db::Database;
use microclaw::memory::MemoryManager;
use microclaw::memory_quality::{
    extract_explicit_memory_command, memory_quality_ok, memory_quality_reason, memory_topic_key,
    normalize_memory_content,
};

fn test_dirs() -> (Database, MemoryManager, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("mc_mem_integ_{}", uuid::Uuid::new_v4()));
    let runtime_dir = dir.join("runtime");
    std::fs::create_dir_all(&runtime_dir).unwrap();
    let db = Database::new(runtime_dir.to_str().unwrap()).unwrap();
    let mm = MemoryManager::new(runtime_dir.to_str().unwrap());
    (db, mm, dir)
}

fn cleanup(dir: &std::path::Path) {
    let _ = std::fs::remove_dir_all(dir);
}

// -----------------------------------------------------------------------
// File memory (AGENTS.md) lifecycle
// -----------------------------------------------------------------------

#[test]
fn test_file_memory_global_write_read_cycle() {
    let (_db, mm, dir) = test_dirs();
    assert!(mm.read_global_memory().is_none());
    mm.write_global_memory("# Global Notes\n- Always use UTC timestamps")
        .unwrap();
    let content = mm.read_global_memory().unwrap();
    assert!(content.contains("Always use UTC timestamps"));
    cleanup(&dir);
}

#[test]
fn test_file_memory_per_chat_isolation() {
    let (_db, mm, dir) = test_dirs();
    mm.write_chat_memory(100, "Chat 100 notes").unwrap();
    mm.write_chat_memory(200, "Chat 200 notes").unwrap();

    let c100 = mm.read_chat_memory(100).unwrap();
    let c200 = mm.read_chat_memory(200).unwrap();
    assert!(c100.contains("Chat 100"));
    assert!(c200.contains("Chat 200"));
    assert!(!c100.contains("Chat 200"));
    assert!(!c200.contains("Chat 100"));

    // Chat 300 never written
    assert!(mm.read_chat_memory(300).is_none());
    cleanup(&dir);
}

#[test]
fn test_file_memory_context_building() {
    let (_db, mm, dir) = test_dirs();
    mm.write_global_memory("Global: team uses PostgreSQL").unwrap();
    mm.write_chat_memory(42, "Chat 42: user prefers Rust").unwrap();

    let ctx = mm.build_memory_context(42);
    assert!(ctx.contains("<global_memory>"));
    assert!(ctx.contains("team uses PostgreSQL"));
    assert!(ctx.contains("</global_memory>"));
    assert!(ctx.contains("<chat_memory>"));
    assert!(ctx.contains("user prefers Rust"));
    assert!(ctx.contains("</chat_memory>"));

    // Chat without its own memory only gets global
    let ctx_other = mm.build_memory_context(999);
    assert!(ctx_other.contains("<global_memory>"));
    assert!(!ctx_other.contains("<chat_memory>"));
    cleanup(&dir);
}

#[test]
fn test_file_memory_overwrite_replaces_content() {
    let (_db, mm, dir) = test_dirs();
    mm.write_chat_memory(1, "Version 1").unwrap();
    assert_eq!(mm.read_chat_memory(1).unwrap(), "Version 1");
    mm.write_chat_memory(1, "Version 2 (updated)").unwrap();
    assert_eq!(mm.read_chat_memory(1).unwrap(), "Version 2 (updated)");
    cleanup(&dir);
}

#[test]
fn test_file_memory_empty_content_excluded_from_context() {
    let (_db, mm, dir) = test_dirs();
    mm.write_global_memory("   \n  \n  ").unwrap();
    mm.write_chat_memory(1, "").unwrap();
    let ctx = mm.build_memory_context(1);
    assert!(!ctx.contains("<global_memory>"), "empty global should be excluded");
    assert!(!ctx.contains("<chat_memory>"), "empty chat should be excluded");
    cleanup(&dir);
}

#[test]
fn test_file_memory_unicode_content() {
    let (_db, mm, dir) = test_dirs();
    mm.write_chat_memory(1, "用户偏好: 使用中文回复, 喜欢简洁风格 🎉")
        .unwrap();
    let content = mm.read_chat_memory(1).unwrap();
    assert!(content.contains("用户偏好"));
    assert!(content.contains("🎉"));
    cleanup(&dir);
}

// -----------------------------------------------------------------------
// Structured memory (SQLite) lifecycle
// -----------------------------------------------------------------------

#[test]
fn test_structured_memory_insert_and_retrieve() {
    let (db, _mm, dir) = test_dirs();
    db.insert_memory(Some(100), "User prefers Rust", "PROFILE")
        .unwrap();
    db.insert_memory(Some(100), "Team uses PostgreSQL on port 5433", "KNOWLEDGE")
        .unwrap();

    let mems = db.get_all_memories_for_chat(Some(100)).unwrap();
    assert_eq!(mems.len(), 2);
    assert!(mems.iter().any(|m| m.content.contains("Rust")));
    assert!(mems.iter().any(|m| m.content.contains("PostgreSQL")));
    cleanup(&dir);
}

#[test]
fn test_structured_memory_cross_chat_isolation() {
    let (db, _mm, dir) = test_dirs();
    db.insert_memory(Some(100), "Chat 100 fact", "KNOWLEDGE")
        .unwrap();
    db.insert_memory(Some(200), "Chat 200 fact", "KNOWLEDGE")
        .unwrap();

    let m100 = db.get_all_memories_for_chat(Some(100)).unwrap();
    let m200 = db.get_all_memories_for_chat(Some(200)).unwrap();
    assert_eq!(m100.len(), 1);
    assert_eq!(m200.len(), 1);
    assert!(m100[0].content.contains("Chat 100"));
    assert!(m200[0].content.contains("Chat 200"));
    cleanup(&dir);
}

#[test]
fn test_structured_memory_archive_and_filter() {
    let (db, _mm, dir) = test_dirs();
    let id1 = db
        .insert_memory(Some(100), "Active fact", "KNOWLEDGE")
        .unwrap();
    let id2 = db
        .insert_memory(Some(100), "Stale fact to archive", "EVENT")
        .unwrap();

    db.archive_memory(id2).unwrap();
    let all = db.get_all_memories_for_chat(Some(100)).unwrap();
    let active: Vec<_> = all.iter().filter(|m| !m.is_archived).collect();
    let archived: Vec<_> = all.iter().filter(|m| m.is_archived).collect();

    assert_eq!(active.len(), 1);
    assert_eq!(archived.len(), 1);
    assert_eq!(active[0].id, id1);
    assert_eq!(archived[0].id, id2);
    cleanup(&dir);
}

#[test]
fn test_structured_memory_with_metadata() {
    let (db, _mm, dir) = test_dirs();
    let id = db
        .insert_memory_with_metadata(
            Some(100),
            "DB port is 5433",
            "KNOWLEDGE",
            "explicit",
            0.95,
        )
        .unwrap();

    let mems = db.get_all_memories_for_chat(Some(100)).unwrap();
    assert_eq!(mems.len(), 1);
    assert_eq!(mems[0].id, id);
    assert!(mems[0].content.contains("5433"));
    assert_eq!(mems[0].category, "KNOWLEDGE");
    cleanup(&dir);
}

#[test]
fn test_structured_memory_supersede_creates_edge_and_archives() {
    let (db, _mm, dir) = test_dirs();
    let old_id = db
        .insert_memory_with_metadata(Some(100), "DB port is 5432", "KNOWLEDGE", "explicit", 0.90)
        .unwrap();

    db.supersede_memory(old_id, "DB port is 5433", "KNOWLEDGE", "explicit", 0.95, None)
        .unwrap();
    let all = db.get_all_memories_for_chat(Some(100)).unwrap();
    let old = all.iter().find(|m| m.id == old_id).unwrap();
    // The new memory was created by supersede_memory; find the non-old one
    let new = all.iter().find(|m| m.id != old_id && m.content.contains("5433")).unwrap();
    assert!(old.is_archived, "old memory should be archived after supersede");
    assert!(!new.is_archived, "new memory should remain active");
    cleanup(&dir);
}

// -----------------------------------------------------------------------
// Dual-layer memory persistence (file + structured combined)
// -----------------------------------------------------------------------

#[test]
fn test_dual_layer_memory_persistence_across_restart() {
    let dir = std::env::temp_dir().join(format!("mc_mem_restart_{}", uuid::Uuid::new_v4()));
    let runtime_dir = dir.join("runtime");
    std::fs::create_dir_all(&runtime_dir).unwrap();

    // Session 1: write both layers
    {
        let db = Database::new(runtime_dir.to_str().unwrap()).unwrap();
        let mm = MemoryManager::new(runtime_dir.to_str().unwrap());
        mm.write_global_memory("# Global\nTimezone: UTC").unwrap();
        mm.write_chat_memory(42, "User prefers bullet points").unwrap();
        db.insert_memory(Some(42), "User's DB port is 5433", "KNOWLEDGE")
            .unwrap();
    }

    // Session 2: verify both layers survive restart (new instances)
    {
        let db = Database::new(runtime_dir.to_str().unwrap()).unwrap();
        let mm = MemoryManager::new(runtime_dir.to_str().unwrap());

        // File memory persists
        let global = mm.read_global_memory().unwrap();
        assert!(global.contains("Timezone: UTC"));
        let chat = mm.read_chat_memory(42).unwrap();
        assert!(chat.contains("bullet points"));

        // Structured memory persists
        let mems = db.get_all_memories_for_chat(Some(42)).unwrap();
        assert_eq!(mems.len(), 1);
        assert!(mems[0].content.contains("5433"));

        // Context building combines both layers
        let ctx = mm.build_memory_context(42);
        assert!(ctx.contains("Timezone: UTC"));
        assert!(ctx.contains("bullet points"));
    }

    cleanup(&dir);
}

// -----------------------------------------------------------------------
// Memory quality gate
// -----------------------------------------------------------------------

#[test]
fn test_quality_gate_accepts_factual_statements() {
    let good = vec![
        "Production database runs on port 5433",
        "Team standups are at 10am UTC daily",
        "User prefers Python over JavaScript",
        "Release cadence is bi-weekly on Fridays",
        "The API rate limit is 100 requests per minute",
    ];
    for text in good {
        assert!(
            memory_quality_ok(text),
            "expected quality OK for: {text}"
        );
    }
}

#[test]
fn test_quality_gate_rejects_low_signal() {
    let bad = vec![
        ("hi", "too short"),
        ("hello", "too short"),
        ("thanks", "too short"),
        ("ok", "too short"),
        ("haha", "too short"),
        ("lol", "too short"),
        ("maybe try python", "uncertain statement"),
        ("I think it could work", "uncertain statement"),
        ("not sure about this", "uncertain statement"),
        ("ab", "too short"),
        ("!!!", "too short"),
        ("!@#$%^&*()!@#$%^&*()", "no signal"),
    ];
    for (text, expected_reason) in bad {
        let result = memory_quality_reason(text);
        assert!(
            result.is_err(),
            "expected rejection for: {text}"
        );
        assert_eq!(
            result.unwrap_err(),
            expected_reason,
            "wrong rejection reason for: {text}"
        );
    }
}

#[test]
fn test_normalize_memory_content_whitespace_handling() {
    let result = normalize_memory_content("  User   prefers   Rust   and   Go  ", 500);
    assert_eq!(result.unwrap(), "User prefers Rust and Go");
}

#[test]
fn test_normalize_memory_content_truncation() {
    let long_text = "a".repeat(500);
    let result = normalize_memory_content(&long_text, 100);
    assert!(result.unwrap().len() <= 100);
}

#[test]
fn test_normalize_memory_content_empty_input() {
    assert!(normalize_memory_content("", 500).is_none());
    assert!(normalize_memory_content("   ", 500).is_none());
}

#[test]
fn test_explicit_memory_command_extraction() {
    // English
    assert_eq!(
        extract_explicit_memory_command("Remember that DB port is 5433"),
        Some("DB port is 5433".to_string())
    );
    assert_eq!(
        extract_explicit_memory_command("Remember this: user likes dark mode"),
        Some("user likes dark mode".to_string())
    );
    assert_eq!(
        extract_explicit_memory_command("memo: deploy to production on Fridays"),
        Some("deploy to production on Fridays".to_string())
    );

    // Chinese
    assert_eq!(
        extract_explicit_memory_command("记住：下周三发布新版本"),
        Some("下周三发布新版本".to_string())
    );
    assert_eq!(
        extract_explicit_memory_command("请记住生产环境用5433端口"),
        Some("生产环境用5433端口".to_string())
    );

    // Not a remember command
    assert!(extract_explicit_memory_command("hello there").is_none());
    assert!(extract_explicit_memory_command("what is the weather?").is_none());
}

#[test]
fn test_topic_key_heuristics() {
    assert_eq!(memory_topic_key("Production database port is 5433"), "db_port");
    assert_eq!(memory_topic_key("The DB port was changed to 5434"), "db_port");
    assert_eq!(memory_topic_key("Release deadline is 2026-03-01"), "deadline");
    assert_eq!(memory_topic_key("Project due date is next Friday"), "deadline");
    assert_eq!(memory_topic_key("User timezone is Asia/Shanghai"), "timezone");
    assert_eq!(memory_topic_key("Server IP address is 10.0.0.1"), "server_ip");

    // Generic fallback: first 4 words
    let generic = memory_topic_key("User prefers dark mode in IDE");
    assert!(!generic.is_empty());
    assert!(generic.contains("user"));
}

#[test]
fn test_quality_gate_combined_with_topic_key() {
    // Good memory should pass quality AND produce a topic key
    let content = "Production database port is 5433";
    assert!(memory_quality_ok(content));
    assert_eq!(memory_topic_key(content), "db_port");

    // Bad memory should fail quality gate regardless of topic key
    let bad = "maybe the port is 5433";
    assert!(!memory_quality_ok(bad));
}

// -----------------------------------------------------------------------
// LLM usage tracking
// -----------------------------------------------------------------------

#[test]
fn test_llm_usage_tracking_lifecycle() {
    let (db, _mm, dir) = test_dirs();
    db.log_llm_usage(100, "telegram", "anthropic", "claude-sonnet-4-5-20250929", 1000, 500, "chat")
        .unwrap();
    db.log_llm_usage(100, "telegram", "anthropic", "claude-sonnet-4-5-20250929", 2000, 800, "chat")
        .unwrap();
    db.log_llm_usage(200, "web", "openai", "gpt-4", 500, 200, "chat")
        .unwrap();

    // Summary for chat 100
    let summary = db.get_llm_usage_summary(Some(100)).unwrap();
    assert_eq!(summary.requests, 2);
    assert_eq!(summary.input_tokens, 3000);
    assert_eq!(summary.output_tokens, 1300);
    assert_eq!(summary.total_tokens, 4300);
    assert!(summary.last_request_at.is_some());

    // Global summary
    let global = db.get_llm_usage_summary(None).unwrap();
    assert_eq!(global.requests, 3);
    assert_eq!(global.input_tokens, 3500);
    assert_eq!(global.output_tokens, 1500);

    cleanup(&dir);
}

// -----------------------------------------------------------------------
// Memory observability
// -----------------------------------------------------------------------

#[test]
fn test_memory_observability_reflector_and_injection_logs() {
    let (db, _mm, dir) = test_dirs();
    db.insert_memory_with_metadata(Some(100), "fact one", "KNOWLEDGE", "explicit", 0.90)
        .unwrap();

    let now = chrono::Utc::now();
    let started = now.to_rfc3339();
    let finished = (now + chrono::Duration::seconds(1)).to_rfc3339();
    db.log_reflector_run(100, &started, &finished, 3, 1, 1, 1, "jaccard", true, None)
        .unwrap();
    db.log_memory_injection(100, "keyword", 5, 2, 3, 80)
        .unwrap();

    let summary = db.get_memory_observability_summary(Some(100)).unwrap();
    assert!(summary.total >= 1);
    assert!(summary.active >= 1);
    assert!(summary.reflector_runs_24h >= 1);
    assert!(summary.injection_events_24h >= 1);
    cleanup(&dir);
}
