use super::*;

#[test]
fn build_chat_body_omits_profile_when_absent() {
    let body = build_chat_body("hi", "claude-haiku-4.5", "KIRO_CLI", None, None);
    assert!(body.get("profileArn").is_none());
    assert_eq!(
        body.pointer("/conversationState/currentMessage/userInputMessage/origin")
            .and_then(Value::as_str),
        Some("KIRO_CLI")
    );
    assert_eq!(
        body.pointer("/conversationState/currentMessage/userInputMessage/modelId")
            .and_then(Value::as_str),
        Some("claude-haiku-4.5")
    );
}

#[test]
fn build_chat_body_includes_profile_when_present() {
    let body = build_chat_body(
        "hi",
        "auto",
        "AI_EDITOR",
        Some("cid"),
        Some("arn:aws:codewhisperer:us-east-1:1:profile/X"),
    );
    assert_eq!(
        body.get("profileArn").and_then(Value::as_str),
        Some("arn:aws:codewhisperer:us-east-1:1:profile/X")
    );
    assert_eq!(
        body.pointer("/conversationState/conversationId")
            .and_then(Value::as_str),
        Some("cid")
    );
}

#[test]
fn parse_list_models_reads_default_object_or_string() {
    let v = json!({
        "models": [{"modelId": "claude-haiku-4.5"}],
        "defaultModel": {"modelId": "auto"}
    });
    let parsed = parse_list_models_response(&v);
    assert_eq!(parsed.default_model.as_deref(), Some("auto"));
    assert_eq!(
        parsed.models,
        vec!["auto".to_string(), "claude-haiku-4.5".to_string()]
    );
}

#[test]
fn http_native_session_roundtrip() {
    let encoded = http_native_session_id("abc-123");
    assert_eq!(encoded, "kiro-http:abc-123");
    assert_eq!(parse_http_native_session_id(&encoded), Some("abc-123"));
    assert_eq!(
        parse_http_native_session_id("  kiro-http:abc-123  "),
        Some("abc-123")
    );
    assert_eq!(parse_http_native_session_id("resume-me"), None);
    assert_eq!(parse_http_native_session_id("kiro-http:"), None);
    assert_eq!(parse_http_native_session_id(""), None);
}

#[test]
fn http_resume_from_opts_namespaces() {
    let mut opts = RunOptions::default();
    assert_eq!(http_resume_from_opts(&opts), Some(HttpResume::New));

    opts.native_session_id = Some("kiro-http:cid-9".into());
    assert_eq!(
        http_resume_from_opts(&opts),
        Some(HttpResume::Conversation("cid-9"))
    );

    opts.native_session_id = Some("resume-me".into());
    assert_eq!(http_resume_from_opts(&opts), None);

    opts.native_session_id = Some("  ".into());
    assert_eq!(http_resume_from_opts(&opts), Some(HttpResume::New));
}

#[test]
fn namespaced_id_feeds_build_chat_body() {
    let native = http_native_session_id("conv-xyz");
    let cid = parse_http_native_session_id(&native).expect("http id");
    let body = build_chat_body("hi", "auto", "AI_EDITOR", Some(cid), None);
    assert_eq!(
        body.pointer("/conversationState/conversationId")
            .and_then(Value::as_str),
        Some("conv-xyz")
    );
}

#[test]
fn pool_access_token_does_not_invent_refresh_and_keeps_envelope() {
    let params = super::super::creds::KiroHttpRouteParams {
        region: "eu-west-1".into(),
        profile_arn: Some("arn:aws:codewhisperer:eu-west-1:1:profile/X".into()),
        origin: "AI_EDITOR".into(),
        api_key: false,
    };
    let creds = creds_from_access_token("at-official", Some(&params));
    assert!(creds.refresh_token.is_none());
    assert!(creds.sqlite_token_key.is_none());
    assert_eq!(creds.region, "eu-west-1");
    assert_eq!(
        creds.profile_arn.as_deref(),
        Some("arn:aws:codewhisperer:eu-west-1:1:profile/X")
    );
    assert_eq!(creds.origin, "AI_EDITOR");
    assert!(creds.token_type_header().is_none());
    assert!(
        creds.needs_refresh(),
        "missing expiry still looks refreshable; the pool path must not call ensure_access_token"
    );
}
