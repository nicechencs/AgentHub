use std::path::PathBuf;

use rusqlite::Connection;
use serde_json::json;
use uuid::Uuid;

use super::*;

#[test]
fn truncate_nanoseconds_for_chrono() {
    let s = truncate_frac_to_micros("2026-09-07T01:08:11.937696853Z");
    assert_eq!(s, "2026-09-07T01:08:11.937696Z");
    assert!(parse_expires("2026-09-07T01:08:11.937696853Z").is_some());
}

#[test]
fn api_key_needs_no_refresh() {
    let creds = KiroHttpCreds {
        auth_kind: KiroAuthKind::ApiKey,
        access_token: "ksk_test".into(),
        refresh_token: None,
        expires_at: None,
        region: "us-east-1".into(),
        profile_arn: None,
        client_id: None,
        client_secret: None,
        origin: "AI_EDITOR".into(),
        sqlite_token_key: None,
        source: "env".into(),
    };
    assert!(!creds.needs_refresh());
    assert_eq!(creds.token_type_header(), Some("API_KEY"));
}

fn temp_sqlite_path() -> PathBuf {
    std::env::temp_dir().join(format!("agenthub-kiro-creds-{}.db", Uuid::new_v4()))
}

fn temp_sso_path() -> PathBuf {
    std::env::temp_dir().join(format!("agenthub-kiro-sso-{}.json", Uuid::new_v4()))
}

fn create_auth_db(path: &PathBuf, records: &[(&str, serde_json::Value)]) {
    let conn = Connection::open(path).expect("create sqlite test db");
    conn.execute_batch(
        "CREATE TABLE auth_kv (key TEXT PRIMARY KEY NOT NULL, value TEXT NOT NULL);\
         CREATE TABLE state (key TEXT PRIMARY KEY NOT NULL, value BLOB NOT NULL);",
    )
    .expect("create auth tables");
    for (key, value) in records {
        conn.execute(
            "INSERT INTO auth_kv (key, value) VALUES (?1, ?2)",
            rusqlite::params![key, value.to_string()],
        )
        .expect("insert auth record");
    }
}

#[test]
fn invalid_social_record_falls_back_to_valid_oidc_record() {
    let path = temp_sqlite_path();
    create_auth_db(
        &path,
        &[
            ("kirocli:social:token", json!({"refresh_token": "stale"})),
            (
                "kirocli:odic:token",
                json!({
                    "access_token": "oidc-access",
                    "refresh_token": "oidc-refresh",
                    "region": "us-west-2"
                }),
            ),
        ],
    );

    let creds = load_from_sqlite(&path)
        .expect("sqlite credentials")
        .expect("valid OIDC fallback");

    assert_eq!(creds.access_token, "oidc-access");
    assert_eq!(creds.refresh_token.as_deref(), Some("oidc-refresh"));
    assert_eq!(creds.region, "us-west-2");
    assert_eq!(
        creds.sqlite_token_key.as_deref(),
        Some("kirocli:odic:token")
    );
    assert_eq!(creds.auth_kind, KiroAuthKind::Desktop);

    let _ = std::fs::remove_file(path);
}

#[test]
fn invalid_social_record_allows_sso_fallback() {
    let path = temp_sqlite_path();
    create_auth_db(
        &path,
        &[(
            "kirocli:social:token",
            json!({"refresh_token": "stale", "access_token": ""}),
        )],
    );

    // `load_kiro_http_creds` tries the SSO cache after this returns None.
    // The assertion keeps this test local and deterministic without touching
    // the user's real Kiro paths or credentials.
    assert!(load_from_sqlite(&path)
        .expect("sqlite credentials")
        .is_none());

    let _ = std::fs::remove_file(path);
}

#[test]
fn invalid_social_and_oidc_records_fall_back_to_sso_cache() {
    let sqlite_path = temp_sqlite_path();
    let sso_path = temp_sso_path();
    create_auth_db(
        &sqlite_path,
        &[
            (
                "kirocli:social:token",
                json!({"refresh_token": "stale-social"}),
            ),
            ("kirocli:odic:token", json!({"refresh_token": "stale-oidc"})),
        ],
    );
    std::fs::write(
        &sso_path,
        json!({
            "access_token": "sso-access",
            "refresh_token": "sso-refresh",
            "region": "eu-west-1"
        })
        .to_string(),
    )
    .expect("write SSO fixture");

    let creds = load_from_local_sources(Some(&sqlite_path), Some(&sso_path))
        .expect("local credential sources")
        .expect("valid SSO fallback");

    assert_eq!(creds.access_token, "sso-access");
    assert_eq!(creds.refresh_token.as_deref(), Some("sso-refresh"));
    assert_eq!(creds.region, "eu-west-1");
    assert_eq!(creds.source, "kiro-auth-token.json");

    let _ = std::fs::remove_file(sqlite_path);
    let _ = std::fs::remove_file(sso_path);
}

#[test]
fn invalid_first_oidc_record_falls_back_to_second_oidc_record() {
    let path = temp_sqlite_path();
    create_auth_db(
        &path,
        &[
            (
                "kirocli:odic:token",
                json!({"refresh_token": "stale-first-oidc"}),
            ),
            (
                "codewhisperer:odic:token",
                json!({
                    "access_token": "second-oidc-access",
                    "refresh_token": "second-oidc-refresh"
                }),
            ),
        ],
    );

    let creds = load_from_sqlite(&path)
        .expect("sqlite credentials")
        .expect("valid second OIDC record");

    assert_eq!(creds.access_token, "second-oidc-access");
    assert_eq!(
        creds.sqlite_token_key.as_deref(),
        Some("codewhisperer:odic:token")
    );

    let _ = std::fs::remove_file(path);
}
