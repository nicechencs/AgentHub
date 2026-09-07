use std::path::{Path, PathBuf};

use crate::models::{
    AccountKind, AgentConfig, AgentId, AuthHealth, Capability, CapabilityLevel, DetectResult,
    DetectStatus, ProcessMode, RunOptions,
};

use crate::adapters::detect_binary::well_known_bin_paths;

use super::*;

#[test]
fn build_run_spec_headless_chat() {
    let adapter = KiroAdapter;
    let bin = PathBuf::from("kiro-cli");
    let spec = adapter
        .build_run_spec(&bin, "hello", &RunOptions::default())
        .unwrap();
    assert_eq!(spec.agent, AgentId::Kiro);
    assert_eq!(spec.program, bin);
    assert_eq!(
        spec.args,
        vec!["chat", "--no-interactive", "--wrap", "never", "hello"]
    );
    assert!(spec.env.iter().any(|(k, v)| k == "TERM" && v == "dumb"));
    assert!(spec.env.iter().any(|(k, v)| k == "NO_COLOR" && v == "1"));
}

#[test]
fn build_run_spec_allow_dangerous_adds_trust_all_tools() {
    let adapter = KiroAdapter;
    let mut opts = RunOptions::default();
    opts.allow_dangerous = true;
    let spec = adapter
        .build_run_spec(Path::new("kiro-cli"), "x", &opts)
        .unwrap();
    assert!(spec.args.iter().any(|a| a == "--trust-all-tools"));
    assert_eq!(spec.args.last().map(String::as_str), Some("x"));
}

#[test]
fn write_config_is_fail_closed() {
    let err = KiroAdapter
        .write_config(&AgentConfig {
            agent: AgentId::Kiro,
            raw: serde_json::json!({}),
        })
        .unwrap_err();
    assert_eq!(err.code(), "unsupported");
}

#[test]
fn account_switch_partial_stream_and_resume_partial() {
    assert_eq!(
        KiroAdapter.capability(Capability::AccountSwitch).level,
        CapabilityLevel::Partial
    );
    assert!(KiroAdapter
        .capability(Capability::StructuredStream)
        .is_usable());
    assert!(KiroAdapter
        .capability(Capability::SessionResume)
        .is_usable());
    assert_eq!(
        KiroAdapter.capability(Capability::DangerousMode).level,
        CapabilityLevel::Partial
    );
    assert_eq!(
        KiroAdapter.capability(Capability::StructuredStream).level,
        CapabilityLevel::Partial
    );
    assert_eq!(
        KiroAdapter.capability(Capability::SessionResume).level,
        CapabilityLevel::Partial
    );
    assert_eq!(
        KiroAdapter.capability(Capability::ModelSelect).level,
        CapabilityLevel::Full
    );
}

#[test]
fn usage_capability_is_partial() {
    assert_eq!(
        KiroAdapter.capability(Capability::Usage).level,
        CapabilityLevel::Partial
    );
}

#[test]
fn build_run_spec_chat_auto_adds_v2_stream_json() {
    let mut opts = RunOptions::default();
    opts.process_mode = ProcessMode::Auto;
    let spec = KiroAdapter
        .build_run_spec(Path::new("kiro-cli"), "hello", &opts)
        .unwrap();
    assert!(spec.args.windows(2).any(|w| w == ["--agent-engine", "v2"]));
    assert!(spec
        .args
        .windows(2)
        .any(|w| w == ["--output-format", "stream-json"]));
    assert_eq!(spec.args.last().map(String::as_str), Some("hello"));
}

#[test]
fn build_run_spec_resume_id_before_prompt() {
    let mut opts = RunOptions::default();
    opts.process_mode = ProcessMode::Auto;
    opts.native_session_id = Some("43829d57-18ca-483f-b0df-054a5e1c395e".into());
    let spec = KiroAdapter
        .build_run_spec(Path::new("kiro-cli"), "ok", &opts)
        .unwrap();
    assert!(spec
        .args
        .windows(2)
        .any(|w| w == ["--resume-id", "43829d57-18ca-483f-b0df-054a5e1c395e"]));
    assert_eq!(spec.args.last().map(String::as_str), Some("ok"));
}

#[test]
fn build_run_spec_model_and_effort_before_prompt() {
    let mut opts = RunOptions::default();
    opts.model = Some("claude-haiku-4.5".into());
    opts.effort = Some("medium".into());
    let spec = KiroAdapter
        .build_run_spec(Path::new("kiro-cli"), "ping", &opts)
        .unwrap();
    assert!(spec
        .args
        .windows(2)
        .any(|w| w == ["--model", "claude-haiku-4.5"]));
    assert!(spec.args.windows(2).any(|w| w == ["--effort", "medium"]));
    assert_eq!(spec.args.last().map(String::as_str), Some("ping"));
}

#[test]
fn build_run_spec_model_effort_with_resume() {
    let mut opts = RunOptions::default();
    opts.process_mode = ProcessMode::Auto;
    opts.model = Some("auto".into());
    opts.effort = Some("high".into());
    opts.native_session_id = Some("43829d57-18ca-483f-b0df-054a5e1c395e".into());
    let spec = KiroAdapter
        .build_run_spec(Path::new("kiro-cli"), "again", &opts)
        .unwrap();
    assert!(spec.args.windows(2).any(|w| w == ["--model", "auto"]));
    assert!(spec.args.windows(2).any(|w| w == ["--effort", "high"]));
    assert!(spec
        .args
        .windows(2)
        .any(|w| w == ["--resume-id", "43829d57-18ca-483f-b0df-054a5e1c395e"]));
    assert_eq!(spec.args.last().map(String::as_str), Some("again"));
}

#[test]
fn skills_dir_is_none_until_paths_verified() {
    assert!(KiroAdapter.skills_dir().is_none());
}

#[test]
fn build_api_key_account_pool_only() {
    let acc = KiroAdapter
        .build_api_key_account("kiro-secret-key")
        .unwrap();
    assert_eq!(acc.agent, AgentId::Kiro);
    assert_eq!(acc.kind, AccountKind::ApiKey);
    let err = KiroAdapter.apply_account(&acc).unwrap_err();
    assert_eq!(err.code(), "unsupported");
}

#[test]
fn kiro_status_health_parses_explicit_lines() {
    assert_eq!(
        kiro_status_health("logged in as demo"),
        AuthHealth::Verified
    );
    assert_eq!(
        kiro_status_health("Not authenticated"),
        AuthHealth::NeedsLogin
    );
    assert_eq!(kiro_status_health("something else"), AuthHealth::Unknown);
}

#[test]
fn desktop_ide_copy_does_not_count_as_installed() {
    let mut result = DetectResult {
        agent: AgentId::Kiro,
        status: DetectStatus::NotFound,
        version: None,
        binary_path: None,
        channel: None,
        env_ready: true,
        notes: Vec::new(),
        extra_copies: Vec::new(),
    };
    attach_kiro_desktop_copy(&mut result);
    assert_eq!(result.status, DetectStatus::NotFound);
    assert!(result.binary_path.is_none());
    assert!(result.extra_copies.iter().all(|c| c.kind == "desktop"));
}

#[test]
fn detect_does_not_treat_ide_as_cli() {
    let r = KiroAdapter.detect();
    if r.status == DetectStatus::Installed {
        let path = r.binary_path.as_ref().unwrap();
        assert!(
            path_looks_like_kiro_cli(path),
            "installed path must be kiro-cli: {}",
            path.display()
        );
    } else {
        assert_eq!(r.status, DetectStatus::NotFound);
        assert!(r.binary_path.is_none());
    }
    assert!(
        r.extra_copies.iter().all(|c| c.kind == "desktop"),
        "IDE/desktop must stay extra: {:?}",
        r.extra_copies
    );
}

#[test]
fn well_known_paths_include_localappdata_kiro_cli() {
    let paths = well_known_bin_paths(AgentId::Kiro);
    assert!(
        paths.iter().any(|(p, ch)| {
            *ch == "native"
                && p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                    n.eq_ignore_ascii_case("kiro-cli") || n.eq_ignore_ascii_case("kiro-cli.exe")
                })
        }),
        "must look for kiro-cli, not kiro: {paths:?}"
    );
    #[cfg(windows)]
    {
        let local = std::env::var("LOCALAPPDATA").expect("LOCALAPPDATA");
        let expected = PathBuf::from(local).join("Kiro-Cli").join("kiro-cli.exe");
        assert!(
            paths.iter().any(|(p, _)| p == &expected),
            "must scan %LOCALAPPDATA%\\Kiro-Cli\\kiro-cli.exe so detect works without PATH: {paths:?}"
        );
    }
}

#[test]
fn install_channels_native_only() {
    let channels = KiroAdapter.install_channels();
    assert_eq!(channels.len(), 1);
    assert_eq!(channels[0].id, "native");
    #[cfg(windows)]
    assert!(channels[0]
        .requires
        .contains(&crate::models::RuntimeId::PowerShell));
    #[cfg(not(windows))]
    assert!(
        !channels[0]
            .requires
            .contains(&crate::models::RuntimeId::PowerShell),
        "macOS/Linux native channel must not require PowerShell"
    );
}

#[test]
fn normalize_token_accepts_sqlite_and_sso_shapes() {
    let sqlite = serde_json::json!({
        "access_token": "aoa-live",
        "refresh_token": "aor-live",
        "expires_at": "2026-09-06T15:05:03Z",
        "provider": "Google",
        "profile_arn": "arn:aws:codewhisperer:us-east-1:1:profile/ABC"
    });
    let body = super::auth::normalize_kiro_token(&sqlite).expect("sqlite token");
    assert_eq!(body["provider"], "google");
    assert_eq!(
        body["profile_arn"],
        "arn:aws:codewhisperer:us-east-1:1:profile/ABC"
    );

    let cache = serde_json::json!({
        "accessToken": "aoa-cache",
        "refreshToken": "aor-cache",
        "profileArn": "arn:aws:codewhisperer:us-east-1:1:profile/ABC",
        "expiresAt": "2026-09-06T15:05:03Z",
        "authMethod": "social",
        "provider": "Google"
    });
    let body = super::auth::normalize_kiro_token(&cache).expect("cache token");
    assert_eq!(body["access_token"], "aoa-cache");
    assert_eq!(body["refresh_token"], "aor-cache");
    assert_eq!(body["auth_method"], "social");
}

#[test]
fn read_social_token_from_sqlite_uses_auth_kv() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("data.sqlite3");
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute(
        "CREATE TABLE auth_kv (key TEXT PRIMARY KEY, value TEXT)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO auth_kv (key, value) VALUES (?1, ?2)",
        rusqlite::params![
            "kirocli:social:token",
            r#"{"access_token":"aoa-db","refresh_token":"aor-db","provider":"google","profile_arn":"arn:aws:codewhisperer:us-east-1:1:profile/ABC"}"#,
        ],
    )
    .unwrap();
    drop(conn);

    let body = super::auth::read_social_token_from_sqlite(&path).expect("token in sqlite");
    assert_eq!(body["access_token"], "aoa-db");
    let live = super::auth::live_account_from_token_body(body, "data.sqlite3");
    assert_eq!(live.agent, AgentId::Kiro);
    assert_eq!(live.kind, AccountKind::Oauth);
    assert_eq!(
        super::auth::kiro_identity_label(&live.credentials, live.label_hint.as_deref()).as_deref(),
        Some("arn:aws:codewhisperer:us-east-1:1:profile/ABC")
    );
}

#[test]
fn write_social_token_roundtrips_sqlite() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("data.sqlite3");
    let body = serde_json::json!({
        "access_token": "aoa-write",
        "refresh_token": "aor-write",
        "expires_at": "2026-09-07T00:00:00Z",
        "provider": "google",
        "profile_arn": "arn:aws:codewhisperer:us-east-1:1:profile/ABC"
    });
    super::auth::write_social_token_to_sqlite(&path, &body).unwrap();
    let read = super::auth::read_social_token_from_sqlite(&path).expect("written token");
    assert_eq!(read["access_token"], "aoa-write");
    assert_eq!(read["expires_at"], "2026-09-07T00:00:00Z");
}

#[test]
fn kiro_grant_is_newer_compares_expires_at() {
    let older = serde_json::json!({
        "body": { "expires_at": "2026-09-06T15:00:00Z" }
    });
    let newer = serde_json::json!({
        "body": { "expires_at": "2026-09-06T16:00:00Z" }
    });
    assert!(super::auth::kiro_grant_is_newer(&newer, &older));
    assert!(!super::auth::kiro_grant_is_newer(&older, &newer));
}
