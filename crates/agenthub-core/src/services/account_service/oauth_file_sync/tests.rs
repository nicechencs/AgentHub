use super::*;
use crate::models::{Account, AccountKind, AgentId};
use chrono::{TimeZone, Utc};
use serde_json::json;

fn ts(raw: &str) -> DateTime<Utc> {
    parse_account_timestamp(raw).expect("timestamp")
}

fn row(updated_at: &str, credentials: Value) -> Account {
    Account {
        id: "row-1".into(),
        agent_id: AgentId::Grok,
        kind: AccountKind::Oauth,
        label: "a@example.com".into(),
        credentials,
        extra: json!({ "source": "oauth_pkce" }),
        status: "active".into(),
        is_current: true,
        created_at: updated_at.to_string(),
        updated_at: updated_at.to_string(),
    }
}

fn decide(
    row: &Account,
    file_credentials: &Value,
    file_mtime: DateTime<Utc>,
) -> OauthFileSyncAction {
    decide_oauth_file_sync(OauthFileSyncInput {
        row,
        file_credentials,
        file_kind: row.kind,
        file_mtime,
    })
}

#[test]
fn newer_row_writes_file() {
    let account = row(
        "2026-08-20 12:00:00.000000",
        json!({
            "format": "auth_json",
            "body": {
                "email": "a@example.com",
                "user_id": "uid-1",
                "key": "at-row",
                "refresh_token": "rt-row"
            }
        }),
    );
    let file = json!({
        "format": "auth_json",
        "body": {
            "email": "a@example.com",
            "user_id": "uid-1",
            "key": "at-file",
            "refresh_token": "rt-file"
        }
    });
    assert_eq!(
        decide(&account, &file, ts("2026-08-19 12:00:00.000000")),
        OauthFileSyncAction::WriteFile
    );
}

#[test]
fn newer_file_writes_row() {
    let account = row(
        "2026-08-19 12:00:00.000000",
        json!({
            "format": "auth_json",
            "body": {
                "email": "a@example.com",
                "user_id": "uid-1",
                "key": "at-row",
                "refresh_token": "rt-row"
            }
        }),
    );
    let file = json!({
        "format": "auth_json",
        "body": {
            "email": "a@example.com",
            "user_id": "uid-1",
            "key": "at-file",
            "refresh_token": "rt-file"
        }
    });
    assert_eq!(
        decide(&account, &file, ts("2026-08-20 12:00:00.000000")),
        OauthFileSyncAction::WriteRow
    );
}

#[test]
fn same_rt_access_rotated_newer_wins() {
    let newer = row(
        "2026-08-20 12:00:00.000000",
        json!({
            "refresh_token": "rt-shared",
            "access_token": "at-row",
            "sub": "uid-1"
        }),
    );
    let file = json!({
        "format": "auth_json",
        "body": {
            "user_id": "uid-1",
            "refresh_token": "rt-shared",
            "key": "at-file"
        }
    });
    assert_eq!(
        decide(&newer, &file, ts("2026-08-19 12:00:00.000000")),
        OauthFileSyncAction::WriteFile
    );
    let older = row("2026-08-19 12:00:00.000000", newer.credentials.clone());
    assert_eq!(
        decide(&older, &file, ts("2026-08-20 12:00:00.000000")),
        OauthFileSyncAction::WriteRow
    );
}

#[test]
fn different_identity_never_writes() {
    let account = row(
        "2026-08-20 12:00:00.000000",
        json!({
            "refresh_token": "rt-a",
            "access_token": "at-a",
            "email": "a@example.com",
            "sub": "uid-a"
        }),
    );
    let file = json!({
        "format": "auth_json",
        "body": {
            "email": "b@example.com",
            "user_id": "uid-b",
            "refresh_token": "rt-b",
            "key": "at-b"
        }
    });
    assert_eq!(
        decide(&account, &file, ts("2026-08-01 00:00:00.000000")),
        OauthFileSyncAction::Skip
    );
}

#[test]
fn equal_mtime_different_rt_is_fail_closed() {
    let stamp = "2026-08-20 12:00:00.000000";
    let account = row(
        stamp,
        json!({
            "email": "a@example.com",
            "user_id": "uid-1",
            "refresh_token": "rt-row-secret",
            "access_token": "at-row"
        }),
    );
    let file = json!({
        "email": "a@example.com",
        "user_id": "uid-1",
        "refresh_token": "rt-file-secret",
        "access_token": "at-file"
    });
    let action = decide(&account, &file, ts(stamp));
    assert_eq!(action, OauthFileSyncAction::NeedsAttention);
    let dumped = format!("{action:?}");
    assert!(
        !dumped.contains("rt-row-secret") && !dumped.contains("rt-file-secret"),
        "sync action must not carry raw refresh tokens: {dumped}"
    );
}

#[test]
fn equal_secrets_are_noop_even_when_timestamps_differ() {
    let account = row(
        "2026-08-20 12:00:00.000000",
        json!({
            "email": "a@example.com",
            "refresh_token": "rt-same",
            "access_token": "at-same"
        }),
    );
    let file = json!({
        "format": "auth_json",
        "body": {
            "email": "a@example.com",
            "refresh_token": "rt-same",
            "key": "at-same"
        }
    });
    assert_eq!(
        decide(&account, &file, ts("2026-08-01 00:00:00.000000")),
        OauthFileSyncAction::Noop
    );
}

#[test]
fn same_rt_without_identity_fields_is_same_lineage() {
    let account = row(
        "2026-08-20 12:00:00.000000",
        json!({
            "refresh_token": "rt-copied",
            "access_token": "at-row"
        }),
    );
    let file = json!({
        "refresh_token": "rt-copied",
        "access_token": "at-file"
    });
    assert_eq!(
        decide(&account, &file, ts("2026-08-01 00:00:00.000000")),
        OauthFileSyncAction::WriteFile
    );
}

#[test]
fn key_only_different_identities_are_unknown_lineage() {
    let account = row(
        "2026-08-20 12:00:00.000000",
        json!({
            "format": "auth_json",
            "body": {"email": "a@example.com", "user_id": "uid-a", "key": "at-a"}
        }),
    );
    let file = json!({
        "format": "auth_json",
        "body": {"email": "b@example.com", "user_id": "uid-b", "key": "at-b"}
    });
    assert_eq!(
        decide(&account, &file, ts("2026-08-01 00:00:00.000000")),
        OauthFileSyncAction::Skip
    );
}

#[test]
fn equal_mtime_same_rt_different_access_is_noop() {
    let stamp = "2026-08-20 12:00:00.000000";
    let account = row(
        stamp,
        json!({
            "email": "a@example.com",
            "user_id": "uid-1",
            "refresh_token": "rt-shared",
            "access_token": "at-row"
        }),
    );
    let file = json!({
        "email": "a@example.com",
        "user_id": "uid-1",
        "refresh_token": "rt-shared",
        "key": "at-file"
    });
    assert_eq!(
        decide(&account, &file, ts(stamp)),
        OauthFileSyncAction::Noop
    );
}

#[test]
fn grok_key_without_rt_is_not_treated_as_equal() {
    let account = row(
        "2026-01-02 00:00:00.000000",
        json!({
            "format": "auth_json",
            "body": {"email": "same@example.com", "user_id": "same-user", "key": "grant-b"}
        }),
    );
    let file = json!({
        "format": "auth_json",
        "body": {"email": "same@example.com", "user_id": "same-user", "key": "grant-c"}
    });
    assert_eq!(
        decide(&account, &file, ts("2026-08-23 00:00:00.000000")),
        OauthFileSyncAction::WriteRow
    );
}

#[test]
fn api_key_same_string_is_noop() {
    let mut account = row(
        "2026-08-20 12:00:00.000000",
        json!({ "format": "api_key", "api_key": "sk-same" }),
    );
    account.kind = AccountKind::ApiKey;
    let file = json!({ "format": "api_key", "api_key": "sk-same" });
    assert_eq!(
        decide_oauth_file_sync(OauthFileSyncInput {
            row: &account,
            file_credentials: &file,
            file_kind: AccountKind::ApiKey,
            file_mtime: ts("2026-08-01 00:00:00.000000"),
        }),
        OauthFileSyncAction::Noop
    );
}

#[test]
fn api_key_different_string_follows_mtime() {
    let mut account = row(
        "2026-08-20 12:00:00.000000",
        json!({ "format": "api_key", "api_key": "sk-row" }),
    );
    account.kind = AccountKind::ApiKey;
    let file = json!({ "format": "api_key", "api_key": "sk-file" });
    assert_eq!(
        decide_oauth_file_sync(OauthFileSyncInput {
            row: &account,
            file_credentials: &file,
            file_kind: AccountKind::ApiKey,
            file_mtime: ts("2026-08-01 00:00:00.000000"),
        }),
        OauthFileSyncAction::WriteFile
    );
}

#[test]
fn patch_codex_token_only_body_without_identity_fields() {
    let mut observed = json!({
        "format": "auth_json",
        "body": {
            "auth_mode": "chatgpt",
            "tokens": {
                "access_token": "at-old",
                "refresh_token": "rt-old"
            },
            "last_refresh": "keep-me"
        }
    });
    let source = json!({
        "type": "oauth",
        "access_token": "at-new",
        "refresh_token": "rt-new"
    });
    assert!(patch_oauth_secrets_into_value(&mut observed, &source));
    assert_eq!(observed["body"]["tokens"]["access_token"], "at-new");
    assert_eq!(observed["body"]["tokens"]["refresh_token"], "rt-new");
    assert_eq!(observed["body"]["last_refresh"], "keep-me");
}

#[test]
fn parse_account_timestamp_accepts_pool_and_rfc3339() {
    assert!(parse_account_timestamp("2026-08-21 00:00:00.123456").is_some());
    assert!(parse_account_timestamp("2026-08-21T00:00:00Z").is_some());
    let parsed = parse_account_timestamp("2026-08-21 00:00:00.000000").unwrap();
    assert_eq!(parsed, Utc.with_ymd_and_hms(2026, 8, 21, 0, 0, 0).unwrap());
}
