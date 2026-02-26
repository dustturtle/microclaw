//! Integration tests for the scheduler, LLM types, and session persistence.
//!
//! Tests scheduled task lifecycle, cron next_run calculation, LLM type
//! serialization round-trips, and session persistence across the database.

use microclaw::db::{Database, StoredMessage};
use microclaw::llm_types::{ContentBlock, Message, MessageContent, ResponseContentBlock};

fn test_db() -> (Database, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("mc_sched_integ_{}", uuid::Uuid::new_v4()));
    let db = Database::new(dir.to_str().unwrap()).unwrap();
    (db, dir)
}

fn cleanup(dir: &std::path::Path) {
    let _ = std::fs::remove_dir_all(dir);
}

// -----------------------------------------------------------------------
// Scheduled task full lifecycle
// -----------------------------------------------------------------------

#[test]
fn test_scheduled_task_create_and_list() {
    let (db, dir) = test_db();
    let chat_id = 100;
    let next_run = chrono::Utc::now().to_rfc3339();

    let task_id = db
        .create_scheduled_task(
            chat_id,
            "Send standup reminder to the team",
            "cron",
            "0 0 10 * * *",
            &next_run,
        )
        .unwrap();
    assert!(task_id > 0);

    let tasks = db.get_tasks_for_chat(chat_id).unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].id, task_id);
    assert_eq!(tasks[0].status, "active");
    assert_eq!(tasks[0].schedule_type, "cron");

    cleanup(&dir);
}

#[test]
fn test_scheduled_task_pause_resume_cancel() {
    let (db, dir) = test_db();
    let next_run = chrono::Utc::now().to_rfc3339();

    let task_id = db
        .create_scheduled_task(100, "Test prompt", "once", &next_run, &next_run)
        .unwrap();

    // Pause
    db.update_task_status(task_id, "paused").unwrap();
    let task = db.get_task_by_id(task_id).unwrap().unwrap();
    assert_eq!(task.status, "paused");

    // Resume
    db.update_task_status(task_id, "active").unwrap();
    let task = db.get_task_by_id(task_id).unwrap().unwrap();
    assert_eq!(task.status, "active");

    // Cancel
    db.update_task_status(task_id, "cancelled").unwrap();
    let task = db.get_task_by_id(task_id).unwrap().unwrap();
    assert_eq!(task.status, "cancelled");

    cleanup(&dir);
}

#[test]
fn test_scheduled_task_one_time_vs_cron() {
    let (db, dir) = test_db();
    let next_run = chrono::Utc::now().to_rfc3339();

    // One-time task
    let one_time = db
        .create_scheduled_task(100, "One-time report", "once", &next_run, &next_run)
        .unwrap();

    // Cron task
    let cron_task = db
        .create_scheduled_task(100, "Weekly digest", "cron", "0 0 9 * * MON", &next_run)
        .unwrap();

    let tasks = db.get_tasks_for_chat(100).unwrap();
    assert_eq!(tasks.len(), 2);

    let one = tasks.iter().find(|t| t.id == one_time).unwrap();
    let cron = tasks.iter().find(|t| t.id == cron_task).unwrap();
    assert_eq!(one.schedule_type, "once");
    assert_eq!(cron.schedule_type, "cron");
    assert_eq!(cron.schedule_value, "0 0 9 * * MON");

    cleanup(&dir);
}

#[test]
fn test_scheduled_task_cross_chat_isolation() {
    let (db, dir) = test_db();
    let next_run = chrono::Utc::now().to_rfc3339();

    db.create_scheduled_task(100, "Task A", "once", &next_run, &next_run)
        .unwrap();
    db.create_scheduled_task(200, "Task B", "once", &next_run, &next_run)
        .unwrap();

    let t100 = db.get_tasks_for_chat(100).unwrap();
    let t200 = db.get_tasks_for_chat(200).unwrap();
    assert_eq!(t100.len(), 1);
    assert_eq!(t200.len(), 1);
    assert_eq!(t100[0].prompt, "Task A");
    assert_eq!(t200[0].prompt, "Task B");

    cleanup(&dir);
}

#[test]
fn test_due_task_retrieval() {
    let (db, dir) = test_db();
    let past = (chrono::Utc::now() - chrono::Duration::hours(1)).to_rfc3339();
    let future = (chrono::Utc::now() + chrono::Duration::hours(1)).to_rfc3339();
    let now = chrono::Utc::now().to_rfc3339();

    // Due task (next_run in the past)
    db.create_scheduled_task(100, "Due task", "once", &past, &past)
        .unwrap();
    // Future task (not due yet)
    db.create_scheduled_task(100, "Future task", "once", &future, &future)
        .unwrap();

    let due = db.get_due_tasks(&now).unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].prompt, "Due task");

    cleanup(&dir);
}

#[test]
fn test_task_update_after_run() {
    let (db, dir) = test_db();
    let past = (chrono::Utc::now() - chrono::Duration::hours(1)).to_rfc3339();
    let now = chrono::Utc::now().to_rfc3339();
    let future = (chrono::Utc::now() + chrono::Duration::hours(1)).to_rfc3339();

    // One-shot task: completes after run
    let one_shot = db
        .create_scheduled_task(100, "One-shot", "once", &past, &past)
        .unwrap();
    db.update_task_after_run(one_shot, &now, None).unwrap();
    let task = db.get_task_by_id(one_shot).unwrap().unwrap();
    assert_eq!(task.status, "completed");
    assert!(task.last_run.is_some());

    // Cron task: gets a new next_run
    let cron = db
        .create_scheduled_task(100, "Recurring", "cron", "0 0 10 * * *", &past)
        .unwrap();
    db.update_task_after_run(cron, &now, Some(&future)).unwrap();
    let task = db.get_task_by_id(cron).unwrap().unwrap();
    assert_eq!(task.status, "active");
    assert_eq!(task.next_run, future);

    cleanup(&dir);
}

#[test]
fn test_task_run_logging() {
    let (db, dir) = test_db();
    let next_run = chrono::Utc::now().to_rfc3339();
    let task_id = db
        .create_scheduled_task(100, "Test task", "once", &next_run, &next_run)
        .unwrap();

    let now = chrono::Utc::now();
    let started = now.to_rfc3339();
    let finished = (now + chrono::Duration::seconds(2)).to_rfc3339();

    // Log a successful run
    db.log_task_run(task_id, 100, &started, &finished, 1500, true, Some("Completed successfully"))
        .unwrap();

    // Log a failed run
    let started2 = (now + chrono::Duration::seconds(5)).to_rfc3339();
    let finished2 = (now + chrono::Duration::seconds(6)).to_rfc3339();
    db.log_task_run(task_id, 100, &started2, &finished2, 300, false, Some("API error: timeout"))
        .unwrap();

    let history = db.get_task_run_logs(task_id, 10).unwrap();
    assert_eq!(history.len(), 2);
    // Most recent first
    assert!(!history[0].success);
    assert!(history[1].success);
    assert!(history[0].result_summary.as_ref().unwrap().contains("timeout"));

    cleanup(&dir);
}

// -----------------------------------------------------------------------
// Session persistence
// -----------------------------------------------------------------------

#[test]
fn test_session_save_load_roundtrip() {
    let (db, dir) = test_db();

    // Build a conversation with tool use
    let messages = vec![
        Message {
            role: "user".to_string(),
            content: MessageContent::Text("What's the weather?".to_string()),
        },
        Message {
            role: "assistant".to_string(),
            content: MessageContent::Blocks(vec![ContentBlock::ToolUse {
                id: "call_123".to_string(),
                name: "web_search".to_string(),
                input: serde_json::json!({"query": "weather today"}),
            }]),
        },
        Message {
            role: "user".to_string(),
            content: MessageContent::Blocks(vec![ContentBlock::ToolResult {
                tool_use_id: "call_123".to_string(),
                content: "Sunny, 25°C".to_string(),
                is_error: None,
            }]),
        },
        Message {
            role: "assistant".to_string(),
            content: MessageContent::Text("It's sunny and 25°C today!".to_string()),
        },
    ];

    let json = serde_json::to_string(&messages).unwrap();
    db.save_session(42, &json).unwrap();

    let loaded = db.load_session(42).unwrap();
    assert!(loaded.is_some());
    let (loaded_json, _updated_at) = loaded.unwrap();
    let loaded_messages: Vec<Message> = serde_json::from_str(&loaded_json).unwrap();
    assert_eq!(loaded_messages.len(), 4);

    // Verify tool use block preserved
    if let MessageContent::Blocks(blocks) = &loaded_messages[1].content {
        match &blocks[0] {
            ContentBlock::ToolUse { id, name, .. } => {
                assert_eq!(id, "call_123");
                assert_eq!(name, "web_search");
            }
            _ => panic!("expected ToolUse block"),
        }
    } else {
        panic!("expected Blocks content");
    }

    cleanup(&dir);
}

#[test]
fn test_session_update_and_delete() {
    let (db, dir) = test_db();

    // Save initial session
    let initial = serde_json::to_string(&vec![Message {
        role: "user".to_string(),
        content: MessageContent::Text("hello".to_string()),
    }])
    .unwrap();
    db.save_session(42, &initial).unwrap();

    // Update with more messages
    let updated = serde_json::to_string(&vec![
        Message {
            role: "user".to_string(),
            content: MessageContent::Text("hello".to_string()),
        },
        Message {
            role: "assistant".to_string(),
            content: MessageContent::Text("hi there!".to_string()),
        },
    ])
    .unwrap();
    db.save_session(42, &updated).unwrap();

    let (loaded_json, _) = db.load_session(42).unwrap().unwrap();
    let msgs: Vec<Message> = serde_json::from_str(&loaded_json).unwrap();
    assert_eq!(msgs.len(), 2);

    // Delete session (reset)
    db.delete_session(42).unwrap();
    assert!(db.load_session(42).unwrap().is_none());

    cleanup(&dir);
}

#[test]
fn test_session_cross_chat_isolation() {
    let (db, dir) = test_db();
    let s1 = serde_json::to_string(&vec![Message {
        role: "user".to_string(),
        content: MessageContent::Text("chat 1 message".to_string()),
    }])
    .unwrap();
    let s2 = serde_json::to_string(&vec![Message {
        role: "user".to_string(),
        content: MessageContent::Text("chat 2 message".to_string()),
    }])
    .unwrap();

    db.save_session(1, &s1).unwrap();
    db.save_session(2, &s2).unwrap();

    let (loaded1, _) = db.load_session(1).unwrap().unwrap();
    let (loaded2, _) = db.load_session(2).unwrap().unwrap();
    assert!(loaded1.contains("chat 1"));
    assert!(loaded2.contains("chat 2"));
    assert!(!loaded1.contains("chat 2"));

    cleanup(&dir);
}

// -----------------------------------------------------------------------
// LLM types serialization round-trips
// -----------------------------------------------------------------------

#[test]
fn test_content_block_text_serde() {
    let block = ContentBlock::Text {
        text: "Hello, world!".to_string(),
    };
    let json = serde_json::to_string(&block).unwrap();
    let deserialized: ContentBlock = serde_json::from_str(&json).unwrap();
    match deserialized {
        ContentBlock::Text { text } => assert_eq!(text, "Hello, world!"),
        _ => panic!("expected Text block"),
    }
}

#[test]
fn test_content_block_tool_use_serde() {
    let block = ContentBlock::ToolUse {
        id: "call_abc".to_string(),
        name: "web_search".to_string(),
        input: serde_json::json!({"query": "rust async"}),
    };
    let json = serde_json::to_string(&block).unwrap();
    let deserialized: ContentBlock = serde_json::from_str(&json).unwrap();
    match deserialized {
        ContentBlock::ToolUse { id, name, input } => {
            assert_eq!(id, "call_abc");
            assert_eq!(name, "web_search");
            assert_eq!(input["query"], "rust async");
        }
        _ => panic!("expected ToolUse block"),
    }
}

#[test]
fn test_content_block_tool_result_serde() {
    let block = ContentBlock::ToolResult {
        tool_use_id: "call_abc".to_string(),
        content: "Search results here".to_string(),
        is_error: None,
    };
    let json = serde_json::to_string(&block).unwrap();
    let deserialized: ContentBlock = serde_json::from_str(&json).unwrap();
    match deserialized {
        ContentBlock::ToolResult {
            tool_use_id,
            content,
            is_error,
        } => {
            assert_eq!(tool_use_id, "call_abc");
            assert_eq!(content, "Search results here");
            assert!(is_error.is_none());
        }
        _ => panic!("expected ToolResult block"),
    }
}

#[test]
fn test_content_block_tool_result_error_serde() {
    let block = ContentBlock::ToolResult {
        tool_use_id: "call_xyz".to_string(),
        content: "Connection timeout".to_string(),
        is_error: Some(true),
    };
    let json = serde_json::to_string(&block).unwrap();
    let deserialized: ContentBlock = serde_json::from_str(&json).unwrap();
    match deserialized {
        ContentBlock::ToolResult { is_error, .. } => assert_eq!(is_error, Some(true)),
        _ => panic!("expected ToolResult block"),
    }
}

#[test]
fn test_message_text_content_serde() {
    let msg = Message {
        role: "user".to_string(),
        content: MessageContent::Text("Hello!".to_string()),
    };
    let json = serde_json::to_string(&msg).unwrap();
    let deserialized: Message = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.role, "user");
    match deserialized.content {
        MessageContent::Text(t) => assert_eq!(t, "Hello!"),
        _ => panic!("expected Text content"),
    }
}

#[test]
fn test_message_blocks_content_serde() {
    let msg = Message {
        role: "assistant".to_string(),
        content: MessageContent::Blocks(vec![
            ContentBlock::Text {
                text: "Let me search for that.".to_string(),
            },
            ContentBlock::ToolUse {
                id: "tool_1".to_string(),
                name: "web_search".to_string(),
                input: serde_json::json!({"query": "rust tutorial"}),
            },
        ]),
    };
    let json = serde_json::to_string(&msg).unwrap();
    let deserialized: Message = serde_json::from_str(&json).unwrap();
    match deserialized.content {
        MessageContent::Blocks(blocks) => {
            assert_eq!(blocks.len(), 2);
        }
        _ => panic!("expected Blocks content"),
    }
}

#[test]
fn test_response_content_block_text_deserialization() {
    let json = r#"{"type":"text","text":"Hello from Claude"}"#;
    let block: ResponseContentBlock = serde_json::from_str(json).unwrap();
    match block {
        ResponseContentBlock::Text { text } => assert_eq!(text, "Hello from Claude"),
        _ => panic!("expected Text"),
    }
}

#[test]
fn test_response_content_block_tool_use_deserialization() {
    let json = r#"{"type":"tool_use","id":"toolu_01","name":"web_search","input":{"query":"test"}}"#;
    let block: ResponseContentBlock = serde_json::from_str(json).unwrap();
    match block {
        ResponseContentBlock::ToolUse { id, name, input } => {
            assert_eq!(id, "toolu_01");
            assert_eq!(name, "web_search");
            assert_eq!(input["query"], "test");
        }
        _ => panic!("expected ToolUse"),
    }
}

// -----------------------------------------------------------------------
// Chat identity mapping
// -----------------------------------------------------------------------

#[test]
fn test_chat_identity_stable_across_lookups() {
    let (db, dir) = test_db();

    let id1 = db
        .resolve_or_create_chat_id("telegram", "12345", Some("Test Chat"), "private")
        .unwrap();
    let id2 = db
        .resolve_or_create_chat_id("telegram", "12345", Some("Test Chat"), "private")
        .unwrap();
    assert_eq!(id1, id2, "same external ID should resolve to same internal ID");

    // Different channel, same external ID → different internal ID
    let id3 = db
        .resolve_or_create_chat_id("discord", "12345", Some("Test Chat"), "private")
        .unwrap();
    assert_ne!(id1, id3, "different channels should produce different IDs");

    cleanup(&dir);
}

#[test]
fn test_chat_identity_web_sessions() {
    let (db, dir) = test_db();

    let id1 = db
        .resolve_or_create_chat_id("web", "session-abc", Some("Web Session"), "web")
        .unwrap();
    let id2 = db
        .resolve_or_create_chat_id("web", "session-xyz", Some("Another Session"), "web")
        .unwrap();
    assert_ne!(id1, id2);

    // Same session key returns same ID
    let id1_again = db
        .resolve_or_create_chat_id("web", "session-abc", None, "web")
        .unwrap();
    assert_eq!(id1, id1_again);

    cleanup(&dir);
}

// -----------------------------------------------------------------------
// Message history with chat identity
// -----------------------------------------------------------------------

#[test]
fn test_message_history_with_resolved_chat_ids() {
    let (db, dir) = test_db();

    let chat_id = db
        .resolve_or_create_chat_id("telegram", "99999", Some("History Test"), "private")
        .unwrap();

    // Store messages
    for i in 0..5 {
        db.store_message(&StoredMessage {
            id: format!("msg-{}", i),
            chat_id,
            sender_name: "user".to_string(),
            content: format!("Message {}", i),
            is_from_bot: i % 2 == 0,
            timestamp: chrono::Utc::now().to_rfc3339(),
        })
        .unwrap();
    }

    let messages = db.get_recent_messages(chat_id, 50).unwrap();
    assert_eq!(messages.len(), 5);

    // Verify ordering (oldest first after reverse)
    assert!(messages[0].content.contains("Message 0"));
    assert!(messages[4].content.contains("Message 4"));

    cleanup(&dir);
}
