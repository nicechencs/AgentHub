use super::*;
use crate::models::{AccountKind, RuntimeId};

#[test]
fn build_run_spec_print_mode() {
    let adapter = CursorAdapter;
    let bin = PathBuf::from("agent");
    let opts = RunOptions::default();
    let spec = adapter.build_run_spec(&bin, "hello", &opts).unwrap();
    assert_eq!(spec.agent, AgentId::Cursor);
    assert_eq!(spec.program, bin);
    assert_eq!(spec.args[0], "-p");
    assert_eq!(spec.args[1], "hello");
    assert!(spec.args.iter().any(|a| a == "--output-format"));
    assert!(spec.args.iter().any(|a| a == "text"));
    assert!(!spec.args.iter().any(|a| a == "--force"));
}

#[test]
fn build_run_spec_allow_dangerous_adds_force() {
    let adapter = CursorAdapter;
    let mut opts = RunOptions::default();
    opts.allow_dangerous = true;
    let spec = adapter
        .build_run_spec(Path::new("agent"), "x", &opts)
        .unwrap();
    assert!(spec.args.iter().any(|a| a == "--force"));
}

#[test]
fn install_channels_native_only() {
    let channels = CursorAdapter.install_channels();
    assert_eq!(channels.len(), 1);
    assert_eq!(channels[0].id, "native");
    #[cfg(windows)]
    assert!(channels[0].requires.contains(&RuntimeId::PowerShell));
    #[cfg(not(windows))]
    assert!(
        !channels[0].requires.contains(&RuntimeId::PowerShell),
        "macOS/Linux native channel must not require PowerShell"
    );
}

#[test]
fn skills_dir_is_skills_cursor() {
    let dir = CursorAdapter.skills_dir().expect("skills_dir");
    let s = dir.to_string_lossy().replace('\\', "/");
    assert!(
        s.ends_with("/.cursor/skills-cursor") || s.contains("/skills-cursor"),
        "unexpected skills_dir: {s}"
    );
}

#[test]
fn write_config_is_fail_closed() {
    let err = CursorAdapter
        .write_config(&AgentConfig {
            agent: AgentId::Cursor,
            raw: serde_json::json!({}),
        })
        .unwrap_err();
    assert_eq!(err.code(), "unsupported");
}

#[test]
fn account_switch_disabled() {
    assert!(CursorAdapter
        .capability(crate::models::Capability::AccountSwitch)
        .is_blocked());
}

#[test]
fn path_rejects_grok_agent() {
    let grok = PathBuf::from(r"C:\Users\demo\.grok\bin\agent.exe");
    assert!(path_is_rejected_non_cursor(&grok));
    assert!(!path_looks_like_cursor_agent(&grok));
}

#[test]
fn path_accepts_cursor_agent_tree() {
    let p = PathBuf::from(r"C:\Users\demo\AppData\Local\cursor-agent\agent.cmd");
    assert!(path_looks_like_cursor_agent(&p));
    assert!(!path_is_rejected_non_cursor(&p));
}

#[test]
fn version_heuristics() {
    assert!(version_looks_like_grok("grok 0.2.118 (1e1687c1cf)"));
    assert!(!version_looks_like_cursor("grok 0.2.118 (1e1687c1cf)"));
    assert!(version_looks_like_cursor("2026.04.29-c83a488"));
    assert!(version_looks_like_cursor("cursor-agent 2026.04.29"));
    assert!(!version_looks_like_cursor("my-agent 1.2.3"));
    assert!(!version_looks_like_cursor("v1.0.0-beta"));
}

#[test]
fn extract_version_from_ps1_install_script() {
    let body = r#"
$downloadUrl = 'https://downloads.cursor.com/lab/2026.07.23-e383d2b/'
$version = '2026.07.23-e383d2b'
function Get-Architecture { }
"#;
    assert_eq!(
        extract_latest_version_from_install_script(body).as_deref(),
        Some("2026.07.23-e383d2b")
    );
}

#[test]
fn extract_version_from_bash_install_script() {
    let body = r#"
TEMP_EXTRACT_DIR="$HOME/.local/share/cursor-agent/versions/.tmp-2026.07.23-e383d2b-$(date +%s)"
DOWNLOAD_URL="https://downloads.cursor.com/lab/2026.07.23-e383d2b/${OS}/${ARCH}/agent-cli-package.tar.gz"
FINAL_DIR="$HOME/.local/share/cursor-agent/versions/2026.07.23-e383d2b"
"#;
    assert_eq!(
        extract_latest_version_from_install_script(body).as_deref(),
        Some("2026.07.23-e383d2b")
    );
}

#[test]
fn extract_version_supports_timestamped_build_id() {
    let body = "$version = '2026.08.01-12-30-45-abcdef1'";
    assert_eq!(
        extract_latest_version_from_install_script(body).as_deref(),
        Some("2026.08.01-12-30-45-abcdef1")
    );
}

#[test]
fn build_api_key_account_is_unsupported() {
    let err = CursorAdapter
        .build_api_key_account("cursor-secret-key")
        .unwrap_err();
    assert_eq!(err.code(), "unsupported");
    assert!(err.to_string().contains("不能配置 API Key"));
}

#[test]
fn detect_does_not_treat_grok_agent_as_cursor() {
    // On this developer machine PATH agent is often Grok — must not become Installed.
    let r = CursorAdapter.detect();
    if r.status == DetectStatus::Installed {
        let path = r.binary_path.as_ref().unwrap();
        assert!(
            !path_is_rejected_non_cursor(path),
            "installed path must not be Grok: {}",
            path.display()
        );
        let s = path.to_string_lossy().to_ascii_lowercase();
        assert!(
            s.contains("cursor-agent")
                || path
                    .file_stem()
                    .and_then(|n| n.to_str())
                    .map(|n| n.eq_ignore_ascii_case("cursor-agent"))
                    .unwrap_or(false),
            "unexpected cursor binary path: {}",
            path.display()
        );
    } else {
        // Expected on machines without Cursor Agent CLI (this repo host as of 2026-08).
        assert_eq!(r.status, DetectStatus::NotFound);
        // Notes should mention IDE or firefighting; must not claim installed via Grok.
        assert!(r.binary_path.is_none());
    }
    assert!(
        r.extra_copies.iter().all(|c| c.kind == "desktop"),
        "IDE/desktop must stay extra, never another kind: {:?}",
        r.extra_copies
    );
    assert!(
        r.extra_copies
            .iter()
            .all(|c| r.binary_path.as_ref().is_none_or(|p| p != &c.path)),
        "desktop copy must not be the spawn path: bin={:?} extras={:?}",
        r.binary_path,
        r.extra_copies
    );
}

#[test]
fn desktop_ide_copy_does_not_count_as_installed() {
    let mut result = DetectResult {
        agent: AgentId::Cursor,
        status: DetectStatus::NotFound,
        version: None,
        binary_path: None,
        channel: None,
        env_ready: true,
        notes: Vec::new(),
        extra_copies: Vec::new(),
    };
    attach_cursor_desktop_copy(&mut result);
    assert_eq!(result.status, DetectStatus::NotFound);
    assert!(result.binary_path.is_none());
    assert!(result.channel.is_none());
    assert!(result.extra_copies.iter().all(|c| c.kind == "desktop"));
}

#[test]
fn read_cursor_auth_from_sqlite_uses_item_table() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.vscdb");
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute(
        "CREATE TABLE ItemTable (key TEXT UNIQUE ON CONFLICT REPLACE, value BLOB)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO ItemTable (key, value) VALUES (?1, ?2)",
        rusqlite::params!["cursorAuth/accessToken", "cursor-access-token"],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO ItemTable (key, value) VALUES (?1, ?2)",
        rusqlite::params!["cursorAuth/refreshToken", "cursor-refresh-token"],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO ItemTable (key, value) VALUES (?1, ?2)",
        rusqlite::params!["cursorAuth/cachedEmail", "demo@example.com"],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO ItemTable (key, value) VALUES (?1, ?2)",
        rusqlite::params![
            "cursorAuth/cachedScopedProfile",
            r#"{"displayName":"Demo User"}"#,
        ],
    )
    .unwrap();
    drop(conn);

    let body = super::auth::read_cursor_auth_from_sqlite(&path).expect("token in sqlite");
    assert_eq!(body["access_token"], "cursor-access-token");
    assert_eq!(body["refresh_token"], "cursor-refresh-token");
    assert_eq!(body["email"], "demo@example.com");
    assert_eq!(body["display_name"], "Demo User");
    let live = super::auth::live_account_from_auth(body, "state.vscdb");
    assert_eq!(live.agent, AgentId::Cursor);
    assert_eq!(live.kind, AccountKind::Oauth);
    assert_eq!(live.extra["source"], "state.vscdb");
    assert_eq!(live.extra["cursorLoginKind"], "window");
    assert_eq!(live.credentials["cursorLoginKind"], "window");
    assert_eq!(
        super::auth::cursor_identity_label(&live.credentials, live.label_hint.as_deref())
            .as_deref(),
        Some("demo@example.com")
    );
}

#[test]
fn read_cursor_auth_from_json_maps_camel_case_tokens() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("auth.json");
    std::fs::write(
        &path,
        r#"{"accessToken":"cli-access","refreshToken":"cli-refresh"}"#,
    )
    .unwrap();
    let body = super::auth::read_cursor_auth_from_json(&path).expect("cli token");
    assert_eq!(body["access_token"], "cli-access");
    assert_eq!(body["refresh_token"], "cli-refresh");
    let live = super::auth::live_account_from_auth(body, "auth.json");
    assert_eq!(live.extra["cursorLoginKind"], "cli");
}

#[test]
fn expand_cursor_live_accounts_splits_different_cli_and_window_grants() {
    let snapshot = super::auth::live_account_from_auth(
        serde_json::json!({
            "access_token": "cli-access",
            "refresh_token": "cli-refresh",
            "email": "cli@example.com",
        }),
        "auth.json",
    );
    let mut snapshot = snapshot;
    snapshot.extra["cursorStores"] = serde_json::json!([
        {
            "source": "auth.json",
            "kind": "cli",
            "body": {
                "access_token": "cli-access",
                "refresh_token": "cli-refresh",
                "email": "cli@example.com"
            }
        },
        {
            "source": "state.vscdb",
            "kind": "window",
            "body": {
                "access_token": "window-access",
                "refresh_token": "window-refresh",
                "email": "window@example.com"
            }
        }
    ]);
    let expanded = super::auth::expand_cursor_live_accounts(&snapshot);
    assert_eq!(expanded.len(), 2);
    assert_eq!(expanded[0].extra["cursorLoginKind"], "cli");
    assert_eq!(expanded[1].extra["cursorLoginKind"], "window");
    assert_eq!(expanded[0].credentials["body"]["email"], "cli@example.com");
    assert_eq!(
        expanded[1].credentials["body"]["email"],
        "window@example.com"
    );
}

#[test]
fn expand_cursor_live_accounts_keeps_single_snapshot() {
    let snapshot = super::auth::live_account_from_auth(
        serde_json::json!({
            "access_token": "only-access",
            "email": "only@example.com",
        }),
        "auth.json",
    );
    let expanded = super::auth::expand_cursor_live_accounts(&snapshot);
    assert_eq!(expanded.len(), 1);
    assert_eq!(expanded[0].extra["cursorLoginKind"], "cli");
}

#[test]
fn read_cursor_auth_from_sqlite_unquotes_json_strings() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.vscdb");
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute(
        "CREATE TABLE ItemTable (key TEXT UNIQUE ON CONFLICT REPLACE, value BLOB)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO ItemTable (key, value) VALUES (?1, ?2)",
        rusqlite::params!["cursorAuth/accessToken", r#""quoted-access""#],
    )
    .unwrap();
    drop(conn);

    let body = super::auth::read_cursor_auth_from_sqlite(&path).expect("quoted token");
    assert_eq!(body["access_token"], "quoted-access");
}

#[test]
fn read_cursor_auth_from_sqlite_missing_access_is_none() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.vscdb");
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute(
        "CREATE TABLE ItemTable (key TEXT UNIQUE ON CONFLICT REPLACE, value BLOB)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO ItemTable (key, value) VALUES (?1, ?2)",
        rusqlite::params!["cursorAuth/cachedEmail", "demo@example.com"],
    )
    .unwrap();
    drop(conn);
    assert!(super::auth::read_cursor_auth_from_sqlite(&path).is_none());
}
