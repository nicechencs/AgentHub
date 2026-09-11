use serde_json::json;

use crate::utils::dsh_session_log::{head_meta, is_log_file, model_from_row};

#[test]
fn recognizes_dsh_session_log_names() {
    for name in [
        "session.jsonl",
        "session.jsonl.zstd",
        "session.v1.jsonl.zstd",
        "session.v3.jsonl",
        "SESSION.V12.JSONL.ZSTD",
    ] {
        assert!(is_log_file(std::path::Path::new(name)), "{name}");
    }
    for name in [
        "wire.jsonl",
        "chat_history.jsonl",
        "updates.jsonl",
        "session.v3.json",
        "session.v.jsonl.zstd",
        "session.v3x.jsonl",
        "session.v3.jsonl.zst",
        "sessions",
    ] {
        assert!(!is_log_file(std::path::Path::new(name)), "{name}");
    }
}

#[test]
fn head_meta_reads_title_user_texts_and_message_count() {
    let text = [
        r#"{"type":"session","version":3,"id":"session-abc","cwd":"D:\\work"}"#,
        r#"{"type":"system/message","seq":7,"data":{"message":{"role":"system","content":[{"type":"text","text":"You are an AI agent"}]}}}"#,
        r#"{"type":"user/message","seq":8,"data":{"content":[{"type":"text","text":"修一下历史记录"}],"role":"user"}}"#,
        r#"{"type":"user/message","seq":11,"data":{"content":[{"type":"text","text":"<system-reminder>skills</system-reminder>"}]}}"#,
        r#"{"type":"session/title","seq":14,"data":{"title":"修一下历史记录","source":{"kind":"fallback"}}}"#,
        r#"{"type":"assistant/message","seq":17,"data":{"message":{"role":"assistant","content":[{"type":"reasoning","text":"thinking"},{"type":"text","text":"好"}]},"usage":{"inputTokens":10,"outputTokens":2}}}"#,
        r#"{"type":"session/title","seq":99,"data":{"title":"DSH 历史与用量","source":{"kind":"provider"}}}"#,
    ]
    .join("\n");

    let meta = head_meta(&text);
    // The provider title wins; the fallback truncation is ignored outright.
    assert_eq!(meta.title.as_deref(), Some("DSH 历史与用量"));
    assert_eq!(
        meta.user_texts,
        vec![
            "修一下历史记录".to_string(),
            "<system-reminder>skills</system-reminder>".to_string()
        ]
    );
    assert_eq!(meta.message_count, Some(3));
}

#[test]
fn head_meta_ignores_a_fallback_only_title() {
    // Real fallback titles are hard-truncated first prompts; AgentHub's preview
    // carries the same message untruncated, so the title stays empty.
    let text = [
        r#"{"type":"user/message","seq":8,"data":{"content":[{"type":"text","text":"你是只读调查员。目标：彻底查清这次改动的影响面"}],"role":"user"}}"#,
        r#"{"type":"session/title","seq":9,"data":{"title":"你是只读调查员。目标：彻底","source":{"kind":"fallback"}}}"#,
    ]
    .join("\n");
    let meta = head_meta(&text);
    assert_eq!(meta.title, None);
    assert_eq!(
        meta.user_texts,
        vec!["你是只读调查员。目标：彻底查清这次改动的影响面".to_string()]
    );
}

#[test]
fn head_meta_without_rows_is_empty() {
    let meta = head_meta("");
    assert_eq!(meta, Default::default());
    let meta = head_meta("not json\n{\"type\":\"tool/call\"}\n");
    assert_eq!(meta, Default::default());
}

#[test]
fn model_from_row_reads_header_config_and_message_source() {
    let header = json!({
        "type": "request/header",
        "data": { "header": { "config": { "provider": "deepseek-official", "model": "deepseek-flash" } } }
    });
    assert_eq!(model_from_row(&header).as_deref(), Some("deepseek-flash"));

    let assistant = json!({
        "type": "assistant/message",
        "data": { "message": { "source": { "kind": "model", "provider": "deepseek-official", "model": "deepseek-v4-pro" } } }
    });
    assert_eq!(model_from_row(&assistant).as_deref(), Some("deepseek-v4-pro"));

    let session = json!({"type": "session", "id": "session-abc", "cwd": "D:\\work"});
    assert_eq!(model_from_row(&session), None);
}
