//! Integration tests for security-related functionality.
//!
//! Tests path guard, symlink attack prevention, and sensitive file protection
//! in a real filesystem environment.

use microclaw::tools::path_guard::{check_path, filter_paths, is_blocked};
use std::path::Path;

// -----------------------------------------------------------------------
// Path guard: relative path traversal
// -----------------------------------------------------------------------

#[test]
fn test_path_guard_relative_traversal_to_ssh() {
    // Relative paths that resolve to sensitive locations should be blocked
    assert!(is_blocked(Path::new("../../.ssh/id_rsa")));
    assert!(is_blocked(Path::new("foo/../../.ssh/config")));
}

#[test]
fn test_path_guard_relative_traversal_to_env() {
    assert!(is_blocked(Path::new("../../.env")));
    assert!(is_blocked(Path::new("subdir/../.env.production")));
}

#[test]
fn test_path_guard_relative_traversal_to_credentials() {
    assert!(is_blocked(Path::new("../credentials.json")));
    assert!(is_blocked(Path::new("config/../token.json")));
}

// -----------------------------------------------------------------------
// Path guard: edge cases
// -----------------------------------------------------------------------

#[test]
fn test_path_guard_dot_files_allowed() {
    // Non-sensitive dot files should be allowed
    assert!(!is_blocked(Path::new(".gitignore")));
    assert!(!is_blocked(Path::new(".editorconfig")));
    assert!(!is_blocked(Path::new(".cargo/config.toml")));
    assert!(!is_blocked(Path::new(".vscode/settings.json")));
}

#[test]
fn test_path_guard_deeply_nested_sensitive_file() {
    assert!(is_blocked(Path::new(
        "/a/b/c/d/e/f/.ssh/authorized_keys"
    )));
    assert!(is_blocked(Path::new("/a/b/c/.aws/config")));
}

#[test]
fn test_path_guard_common_project_files_allowed() {
    let safe_paths = vec![
        "Cargo.toml",
        "src/main.rs",
        "tests/test_integration.rs",
        "README.md",
        "package.json",
        "web/src/App.tsx",
        "data/output.csv",
        "logs/app.log",
    ];
    for p in safe_paths {
        assert!(
            !is_blocked(Path::new(p)),
            "expected safe: {p}"
        );
        assert!(
            check_path(p).is_ok(),
            "check_path should pass for: {p}"
        );
    }
}

#[test]
fn test_path_guard_all_blocked_files_individually() {
    let blocked = vec![
        ".env",
        ".env.local",
        ".env.production",
        ".env.development",
        "credentials",
        "credentials.json",
        "token.json",
        "secrets.yaml",
        "secrets.json",
        "id_rsa",
        "id_rsa.pub",
        "id_ed25519",
        "id_ed25519.pub",
        "id_ecdsa",
        "id_ecdsa.pub",
        "id_dsa",
        "id_dsa.pub",
        ".netrc",
        ".npmrc",
    ];
    for f in blocked {
        let path = format!("/project/{}", f);
        assert!(
            is_blocked(Path::new(&path)),
            "expected blocked: {path}"
        );
    }
}

#[test]
fn test_path_guard_all_blocked_dirs() {
    let blocked_dirs = vec![".ssh", ".aws", ".gnupg", ".kube"];
    for d in blocked_dirs {
        let path = format!("/home/user/{}/some_file", d);
        assert!(
            is_blocked(Path::new(&path)),
            "expected blocked dir: {path}"
        );
    }
}

#[test]
fn test_path_guard_blocked_absolute_paths() {
    let abs_blocked = vec!["/etc/shadow", "/etc/gshadow", "/etc/sudoers"];
    for p in abs_blocked {
        assert!(
            is_blocked(Path::new(p)),
            "expected blocked absolute: {p}"
        );
    }
}

#[test]
fn test_path_guard_gcloud_subpath() {
    assert!(is_blocked(Path::new("/home/user/.config/gcloud/credentials.db")));
    assert!(is_blocked(Path::new("/home/user/.config/gcloud/application_default_credentials.json")));
    // .config alone is fine
    assert!(!is_blocked(Path::new("/home/user/.config/some_app/config.toml")));
}

// -----------------------------------------------------------------------
// Symlink attack prevention
// -----------------------------------------------------------------------

#[test]
fn test_path_guard_symlink_to_sensitive_path() {
    let dir = std::env::temp_dir().join(format!("mc_symlink_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();

    // Create a real sensitive directory structure and a symlink to it
    let ssh_dir = dir.join(".ssh");
    std::fs::create_dir_all(&ssh_dir).unwrap();
    std::fs::write(ssh_dir.join("id_rsa"), "FAKE_KEY").unwrap();

    // Create symlink: dir/link -> dir/.ssh
    let link_path = dir.join("innocent_link");
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&ssh_dir, &link_path).unwrap();
        // The symlink target resolves to .ssh, which should be blocked
        assert!(
            is_blocked(&link_path.join("id_rsa")),
            "symlink to .ssh should be blocked"
        );
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_path_guard_symlink_to_env_file() {
    let dir = std::env::temp_dir().join(format!("mc_symlink_env_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();

    // Create actual .env
    let env_file = dir.join(".env");
    std::fs::write(&env_file, "SECRET=value").unwrap();

    // Create symlink: dir/config.txt -> dir/.env
    let link_path = dir.join("config.txt");
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&env_file, &link_path).unwrap();
        // The symlink resolves to .env, which should be blocked
        assert!(
            is_blocked(&link_path),
            "symlink to .env should be blocked"
        );
    }

    let _ = std::fs::remove_dir_all(&dir);
}

// -----------------------------------------------------------------------
// Filter paths integration
// -----------------------------------------------------------------------

#[test]
fn test_filter_paths_mixed_safe_and_sensitive() {
    let paths = vec![
        "src/main.rs".to_string(),
        "/home/user/.ssh/id_rsa".to_string(),
        "README.md".to_string(),
        "/project/.env".to_string(),
        "Cargo.toml".to_string(),
        "/home/user/.aws/credentials".to_string(),
        "tests/test_db.rs".to_string(),
        "/project/secrets.yaml".to_string(),
        "/etc/shadow".to_string(),
    ];

    let filtered = filter_paths(paths);
    assert_eq!(filtered.len(), 4);
    assert!(filtered.contains(&"src/main.rs".to_string()));
    assert!(filtered.contains(&"README.md".to_string()));
    assert!(filtered.contains(&"Cargo.toml".to_string()));
    assert!(filtered.contains(&"tests/test_db.rs".to_string()));
}

#[test]
fn test_filter_paths_all_safe() {
    let paths = vec![
        "src/lib.rs".to_string(),
        "tests/integration.rs".to_string(),
    ];
    let filtered = filter_paths(paths.clone());
    assert_eq!(filtered.len(), paths.len());
}

#[test]
fn test_filter_paths_all_blocked() {
    let paths = vec![
        "/home/.ssh/id_rsa".to_string(),
        "/project/.env".to_string(),
    ];
    let filtered = filter_paths(paths);
    assert!(filtered.is_empty());
}

#[test]
fn test_filter_paths_empty_input() {
    let filtered = filter_paths(vec![]);
    assert!(filtered.is_empty());
}

// -----------------------------------------------------------------------
// check_path API
// -----------------------------------------------------------------------

#[test]
fn test_check_path_returns_descriptive_error() {
    let result = check_path("/home/user/.ssh/id_rsa");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.contains("Access denied"));
    assert!(err.contains(".ssh/id_rsa"));
}

#[test]
fn test_check_path_ok_for_safe_paths() {
    assert!(check_path("src/main.rs").is_ok());
    assert!(check_path("Cargo.toml").is_ok());
    assert!(check_path("/tmp/test.txt").is_ok());
}
