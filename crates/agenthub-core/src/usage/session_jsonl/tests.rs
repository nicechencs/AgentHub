use super::*;
use crate::platform::usage::parse_file_for_agent_id;
use crate::storage::Database;
use tempfile::tempdir;

#[test]
fn extracts_ccusage_claude_fixture_shape() {
    // From ccusage apps/ccusage/test/fixtures/claude/.../chat.jsonl
    let line = r#"{"timestamp":"2026-01-09T10:00:00.000Z","sessionId":"session-alpha","message":{"role":"assistant","id":"msg-alpha-1","model":"claude-sonnet-4-20250514","usage":{"input_tokens":100,"output_tokens":50,"cache_creation_input_tokens":25,"cache_read_input_tokens":10}},"requestId":"req-alpha-1","costUSD":0.12,"version":"1.0.0"}"#;
    let ev = extract_claude_like(AgentId::Claude, line, Some("sess1"))
        .unwrap()
        .unwrap();
    assert_eq!(ev.input_tokens, 100);
    assert_eq!(ev.output_tokens, 50);
    assert_eq!(ev.cache_creation_tokens, 25);
    assert_eq!(ev.cache_read_tokens, 10);
    assert_eq!(ev.model, "claude-sonnet-4-20250514");
    assert_eq!(ev.cost_usd, Some(0.12));
    assert_eq!(ev.session_id.as_deref(), Some("session-alpha"));
    // message_id+request_id dedupe (not full line hash only)
    assert!(!ev.raw_hash.is_empty());
}

#[test]
fn extracts_ccusage_codex_token_count() {
    // When model is present on the token_count info itself
    let line = r#"{"timestamp":"2026-05-13T09:01:00.000Z","type":"event_msg","payload":{"type":"token_count","info":{"model":"gpt-5.2-codex","last_token_usage":{"input_tokens":1000,"cached_input_tokens":250,"output_tokens":125,"reasoning_output_tokens":75,"total_tokens":1200}}}}"#;
    let mut state = CodexParseState::default();
    let ev = extract_codex(line, Some("s1"), &mut state)
        .unwrap()
        .unwrap();
    // Stored input is non-cached (ccusage inputTokens), cache kept separate
    assert_eq!(ev.input_tokens, 750);
    // reasoning is informational; output_tokens already bills it (ccusage)
    assert_eq!(ev.output_tokens, 125);
    assert_eq!(ev.cache_read_tokens, 250);
    assert_eq!(ev.model, "gpt-5.2-codex");
}

#[test]
fn codex_skips_generic_usage_that_would_not_peel_cache() {
    let mut state = CodexParseState::default();
    let line = r#"{"timestamp":"2026-05-13T09:01:00.000Z","type":"response","usage":{"input_tokens":1000,"cached_input_tokens":250,"output_tokens":10}}"#;
    assert!(extract_codex(line, Some("s1"), &mut state)
        .unwrap()
        .is_none());
}

#[test]
fn codex_inherits_model_from_turn_context() {
    // Real Codex logs: model only on turn_context; token_count has none.
    let mut state = CodexParseState::default();
    let ctx = r#"{"timestamp":"2026-08-03T03:12:03.326Z","type":"turn_context","payload":{"model":"gpt-5.6-sol"}}"#;
    assert!(extract_codex(ctx, Some("s1"), &mut state)
        .unwrap()
        .is_none());
    assert_eq!(state.model.as_deref(), Some("gpt-5.6-sol"));

    let tok = r#"{"timestamp":"2026-08-03T03:12:09.556Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":21650,"cached_input_tokens":11008,"output_tokens":217,"reasoning_output_tokens":60,"total_tokens":21867},"last_token_usage":{"input_tokens":21650,"cached_input_tokens":11008,"output_tokens":217,"reasoning_output_tokens":60,"total_tokens":21867},"model_context_window":258400}}}"#;
    let ev = extract_codex(tok, Some("s1"), &mut state).unwrap().unwrap();
    assert_eq!(ev.model, "gpt-5.6-sol");
    assert_eq!(ev.input_tokens, 21650 - 11008);
    assert_eq!(ev.output_tokens, 217);
    assert_eq!(ev.cache_read_tokens, 11008);
}

#[test]
fn codex_falls_back_to_unknown_without_model() {
    // Never invent gpt-5 — real model comes from turn_context or config.toml.
    let mut state = CodexParseState::default();
    let tok = r#"{"timestamp":"2026-05-13T09:01:00.000Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":10,"cached_input_tokens":0,"output_tokens":5,"reasoning_output_tokens":0,"total_tokens":15}}}}"#;
    let ev = extract_codex(tok, None, &mut state).unwrap().unwrap();
    assert_eq!(ev.model, "unknown");
    assert!(state.model.is_none());
}

#[test]
fn codex_uses_config_seeded_model_when_token_count_has_none() {
    let mut state = CodexParseState::with_model(Some("gpt-5.6-luna".into()));
    let tok = r#"{"timestamp":"2026-05-13T09:01:00.000Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":10,"cached_input_tokens":0,"output_tokens":5,"reasoning_output_tokens":0,"total_tokens":15}}}}"#;
    let ev = extract_codex(tok, None, &mut state).unwrap().unwrap();
    assert_eq!(ev.model, "gpt-5.6-luna");
}

#[test]
fn codex_skips_duplicate_total_token_usage() {
    // ccusage: unchanged total_token_usage → drop last_token_usage (issue #884).
    let mut state = CodexParseState::default();
    let a = r#"{"timestamp":"2026-05-13T09:01:00.000Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":100,"cached_input_tokens":0,"output_tokens":10,"total_tokens":110},"last_token_usage":{"input_tokens":100,"cached_input_tokens":0,"output_tokens":10,"total_tokens":110}}}}"#;
    let b = r#"{"timestamp":"2026-05-13T09:01:01.000Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":100,"cached_input_tokens":0,"output_tokens":10,"total_tokens":110},"last_token_usage":{"input_tokens":100,"cached_input_tokens":0,"output_tokens":10,"total_tokens":110}}}}"#;
    assert!(extract_codex(a, None, &mut state).unwrap().is_some());
    assert!(extract_codex(b, None, &mut state).unwrap().is_none());
    assert_eq!(state.skipped_dup_total, 1);
    assert_eq!(state.emitted, 1);
}

#[test]
fn codex_total_delta_when_last_token_usage_missing() {
    let mut state = CodexParseState::default();
    let first = r#"{"timestamp":"2026-05-13T09:01:00.000Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":100,"cached_input_tokens":20,"output_tokens":5,"total_tokens":105}}}}"#;
    let second = r#"{"timestamp":"2026-05-13T09:01:05.000Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":250,"cached_input_tokens":50,"output_tokens":15,"total_tokens":265}}}}"#;
    let e1 = extract_codex(first, None, &mut state).unwrap().unwrap();
    // First total with no last → bill whole total (non-cached).
    assert_eq!(e1.input_tokens, 80);
    assert_eq!(e1.cache_read_tokens, 20);
    let e2 = extract_codex(second, None, &mut state).unwrap().unwrap();
    // Delta: input 150, cached 30 → non-cached 120
    assert_eq!(e2.input_tokens, 120);
    assert_eq!(e2.cache_read_tokens, 30);
    assert_eq!(e2.output_tokens, 10);
}

#[test]
fn codex_fork_rewritten_burst_skipped_then_child_counted() {
    // Dense open (same second) = parent history replay; pause >1s = child work.
    let mut state = CodexParseState {
        forkish: true,
        burst_skip_active: true,
        replay: CodexReplaySkip::SkippingBurst {
            last_ms: parse_iso_ms("2026-08-07T10:09:15.277Z").unwrap(),
        },
        ..Default::default()
    };
    let burst1 = r#"{"timestamp":"2026-08-07T10:09:15.277Z","type":"event_msg","payload":{"type":"token_count","info":{"model":"gpt-5.6-sol","total_token_usage":{"input_tokens":1000,"cached_input_tokens":900,"output_tokens":10,"total_tokens":1010},"last_token_usage":{"input_tokens":1000,"cached_input_tokens":900,"output_tokens":10,"total_tokens":1010}}}}"#;
    let burst2 = r#"{"timestamp":"2026-08-07T10:09:15.500Z","type":"event_msg","payload":{"type":"token_count","info":{"model":"gpt-5.6-sol","total_token_usage":{"input_tokens":2000,"cached_input_tokens":1800,"output_tokens":20,"total_tokens":2020},"last_token_usage":{"input_tokens":1000,"cached_input_tokens":900,"output_tokens":10,"total_tokens":1010}}}}"#;
    let child = r#"{"timestamp":"2026-08-07T10:09:20.000Z","type":"event_msg","payload":{"type":"token_count","info":{"model":"gpt-5.6-sol","total_token_usage":{"input_tokens":2100,"cached_input_tokens":1900,"output_tokens":30,"total_tokens":2130},"last_token_usage":{"input_tokens":100,"cached_input_tokens":50,"output_tokens":10,"total_tokens":110}}}}"#;

    assert!(extract_codex(burst1, Some("fork"), &mut state)
        .unwrap()
        .is_none());
    assert!(extract_codex(burst2, Some("fork"), &mut state)
        .unwrap()
        .is_none());
    assert!(state.skipped_burst >= 2);

    let ev = extract_codex(child, Some("fork"), &mut state)
        .unwrap()
        .unwrap();
    assert_eq!(ev.input_tokens, 50); // 100 - 50
    assert_eq!(ev.cache_read_tokens, 50);
    assert_eq!(ev.output_tokens, 10);
    assert_eq!(state.emitted, 1);
}

#[test]
fn codex_non_fork_counts_dense_open_events() {
    // Without burst skip, dense timestamps are still real turns.
    let mut state = CodexParseState::default();
    let a = r#"{"timestamp":"2026-08-07T10:09:15.277Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":100,"cached_input_tokens":0,"output_tokens":1,"total_tokens":101},"last_token_usage":{"input_tokens":100,"cached_input_tokens":0,"output_tokens":1,"total_tokens":101}}}}"#;
    let b = r#"{"timestamp":"2026-08-07T10:09:15.500Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":250,"cached_input_tokens":0,"output_tokens":3,"total_tokens":253},"last_token_usage":{"input_tokens":150,"cached_input_tokens":0,"output_tokens":2,"total_tokens":152}}}}"#;
    assert_eq!(
        extract_codex(a, None, &mut state)
            .unwrap()
            .unwrap()
            .input_tokens,
        100
    );
    assert_eq!(
        extract_codex(b, None, &mut state)
            .unwrap()
            .unwrap()
            .input_tokens,
        150
    );
    assert_eq!(state.emitted, 2);
    assert_eq!(state.skipped_burst, 0);
}

#[test]
fn codex_init_from_file_detects_fork_and_burst() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("rollout-fork.jsonl");
    let content = concat!(
        r#"{"timestamp":"2026-08-07T10:09:15.000Z","type":"session_meta","payload":{"id":"child","parent_thread_id":"parent","forked_from_id":"parent"}}"#,
        "\n",
        r#"{"timestamp":"2026-08-07T10:09:15.100Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":10,"cached_input_tokens":0,"output_tokens":1,"total_tokens":11},"last_token_usage":{"input_tokens":10,"cached_input_tokens":0,"output_tokens":1,"total_tokens":11}}}}"#,
        "\n",
        r#"{"timestamp":"2026-08-07T10:09:15.200Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":20,"cached_input_tokens":0,"output_tokens":2,"total_tokens":22},"last_token_usage":{"input_tokens":10,"cached_input_tokens":0,"output_tokens":1,"total_tokens":11}}}}"#,
        "\n",
    );
    fs::write(&path, content).unwrap();
    let state = CodexParseState::init_from_file(&path, None, false);
    assert!(state.forkish);
    assert!(state.burst_skip_active);
    assert!(matches!(
        state.replay,
        CodexReplaySkip::SkippingBurst { .. }
    ));
}

#[test]
fn parse_iso_ms_accepts_z_and_offsets() {
    let a = parse_iso_ms("2026-08-07T10:09:15.277Z").unwrap();
    let b = parse_iso_ms("2026-08-07T10:09:15.277+00:00").unwrap();
    assert_eq!(a, b);
    let c = parse_iso_ms("2026-08-07T10:09:16.277Z").unwrap();
    assert_eq!(c - a, 1000);
    assert!(parse_iso_ms("not-a-date").is_none());
    assert!(parse_iso_ms("").is_none());
}

#[test]
fn read_codex_default_model_from_toml() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("config.toml"),
        "model = \"gpt-5.6-luna\"\nmodel_reasoning_effort = \"xhigh\"\n",
    )
    .unwrap();
    assert_eq!(
        read_codex_default_model(dir.path()).as_deref(),
        Some("gpt-5.6-luna")
    );
}

#[test]
fn read_pi_default_model_from_settings() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("settings.json"),
        r#"{"defaultProvider":"xai","defaultModel":"grok-4.5"}"#,
    )
    .unwrap();
    assert_eq!(
        read_pi_default_model(dir.path()).as_deref(),
        Some("grok-4.5")
    );
}

#[test]
fn skips_non_usage_lines() {
    let line = r#"{"type":"user","message":{"content":"hi"}}"#;
    assert!(extract_claude_like(AgentId::Claude, line, None)
        .unwrap()
        .is_none());
}

#[test]
fn codex_skips_turn_context_without_tokens() {
    let line = r#"{"timestamp":"2026-05-13T09:00:00.000Z","type":"turn_context","payload":{"model":"gpt-5.2-codex"}}"#;
    let mut state = CodexParseState::default();
    assert!(extract_codex(line, None, &mut state).unwrap().is_none());
}

#[test]
fn bootstrap_codex_model_reads_prefix() {
    let dir = tempdir().unwrap();
    let log = dir.path().join("rollout.jsonl");
    let content = concat!(
        r#"{"timestamp":"2026-08-03T03:12:03.326Z","type":"turn_context","payload":{"model":"gpt-5.6-sol"}}"#,
        "\n",
        r#"{"timestamp":"2026-08-03T03:12:09.556Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":1,"output_tokens":1,"cached_input_tokens":0,"reasoning_output_tokens":0,"total_tokens":2}}}}"#,
        "\n",
    );
    fs::write(&log, content).unwrap();
    let first_line_end = content.find('\n').unwrap() as u64 + 1;
    let m = bootstrap_codex_model(&log, first_line_end);
    assert_eq!(m.as_deref(), Some("gpt-5.6-sol"));
}

#[test]
fn bootstrap_codex_prefix_recovers_previous_total_for_resume() {
    let dir = tempdir().unwrap();
    let log = dir.path().join("rollout.jsonl");
    let l1 = r#"{"timestamp":"2026-08-03T03:12:03.326Z","type":"turn_context","payload":{"model":"gpt-5.6-sol"}}"#;
    let l2 = r#"{"timestamp":"2026-08-03T03:12:09.556Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1000,"cached_input_tokens":400,"output_tokens":50,"total_tokens":1050},"last_token_usage":{"input_tokens":1000,"cached_input_tokens":400,"output_tokens":50,"total_tokens":1050}}}}"#;
    let l3 = r#"{"timestamp":"2026-08-03T03:12:20.000Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1000,"cached_input_tokens":400,"output_tokens":50,"total_tokens":1050},"last_token_usage":{"input_tokens":1000,"cached_input_tokens":400,"output_tokens":50,"total_tokens":1050}}}}"#;
    let content = format!("{l1}\n{l2}\n{l3}\n");
    fs::write(&log, &content).unwrap();
    // Resume after first two lines (past the first token_count).
    let up_to = (l1.len() + 1 + l2.len() + 1) as u64;
    let (model, prev, tier) = bootstrap_codex_prefix(&log, up_to);
    assert_eq!(model.as_deref(), Some("gpt-5.6-sol"));
    assert!(tier.is_none());
    let prev = prev.expect("previous total");
    assert_eq!(prev.input, 1000);
    assert_eq!(prev.cached, 400);
    assert_eq!(prev.output, 50);

    // With previous_total seeded, a duplicate total snapshot must be skipped.
    let mut state = CodexParseState::resume_from_prefix(model, Some(prev), None, false);
    assert!(extract_codex(l3, Some("s"), &mut state).unwrap().is_none());
    assert_eq!(state.skipped_dup_total, 1);
}

#[test]
fn extracts_kimi_old_status_update_with_config_hint() {
    let line = r#"{"timestamp":1770983427.123,"message":{"type":"StatusUpdate","payload":{"message_id":"m1","token_usage":{"input_other":100,"output":50,"input_cache_creation":10,"input_cache_read":20,"total":180}}}}"#;
    // Without hint → unknown (never invent kimi-for-coding)
    let bare = extract_kimi(line, Some("sess-a"), None).unwrap().unwrap();
    assert_eq!(bare.model, "unknown");
    // With config/llm hint
    let ev = extract_kimi(line, Some("sess-a"), Some("kimi-code/k3"))
        .unwrap()
        .unwrap();
    assert_eq!(ev.input_tokens, 100);
    assert_eq!(ev.output_tokens, 50);
    assert_eq!(ev.cache_creation_tokens, 10);
    assert_eq!(ev.cache_read_tokens, 20);
    assert_eq!(ev.model, "k3");
}

#[test]
fn extracts_kimi_new_usage_record_from_real_shape() {
    // Real ~/.kimi-code wire.jsonl sample
    let turn = r#"{"type":"usage.record","model":"kimi-code/k3","usage":{"inputOther":8808,"output":2479,"inputCacheRead":19200,"inputCacheCreation":0},"usageScope":"turn","time":1784277702176}"#;
    let ev = extract_kimi(turn, Some("sess-b"), None).unwrap().unwrap();
    assert_eq!(ev.input_tokens, 8808);
    assert_eq!(ev.output_tokens, 2479);
    assert_eq!(ev.cache_creation_tokens, 0);
    assert_eq!(ev.cache_read_tokens, 19200);
    assert_eq!(ev.model, "k3");

    let session =
        r#"{"type":"usage.record","usageScope":"session","usage":{"inputOther":999,"output":999}}"#;
    assert!(extract_kimi(session, None, None).unwrap().is_none());
}

#[test]
fn kimi_inherits_model_from_llm_request() {
    let mut model = None;
    let req = r#"{"type":"llm.request","kind":"loop","provider":"kimi","model":"k3","modelAlias":"kimi-code/k3","time":1784276981665}"#;
    note_kimi_model_from_line(req, &mut model);
    assert_eq!(model.as_deref(), Some("k3"));
    let usage = r#"{"type":"usage.record","usageScope":"turn","usage":{"inputOther":1,"output":2,"inputCacheRead":0,"inputCacheCreation":0},"time":1784276982000}"#;
    // no model on usage line → use inheritance
    let ev = extract_kimi(usage, None, model.as_deref())
        .unwrap()
        .unwrap();
    assert_eq!(ev.model, "k3");
}

#[test]
fn read_kimi_default_model_from_toml() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("config.toml"),
        "default_model = \"kimi-code/k3\"\n\n[thinking]\nenabled = true\n",
    )
    .unwrap();
    assert_eq!(read_kimi_default_model(dir.path()).as_deref(), Some("k3"));
}

#[test]
fn kimi_total_fallback_when_parts_missing() {
    let line = r#"{"timestamp":1770983427.0,"message":{"type":"StatusUpdate","payload":{"token_usage":{"total":432}}}}"#;
    let ev = extract_kimi(line, None, Some("kimi-for-coding"))
        .unwrap()
        .unwrap();
    assert_eq!(ev.output_tokens, 432);
    assert_eq!(ev.input_tokens, 0);
    assert_eq!(ev.model, "kimi-for-coding");
}

#[test]
fn kimi_root_from_new_and_old_wire_paths() {
    let new = PathBuf::from("/home/u/.kimi-code/sessions/wd_x/session_y/agents/main/wire.jsonl");
    let root = kimi_root_from_wire_path(&new).unwrap();
    assert!(root.ends_with(".kimi-code"));
    let old = PathBuf::from("/home/u/.kimi/sessions/group/session/wire.jsonl");
    let root2 = kimi_root_from_wire_path(&old).unwrap();
    assert!(root2.ends_with(".kimi"));
}

#[test]
fn extracts_pi_assistant_usage() {
    // Real ~/.pi shape (observed on this machine)
    let line = r#"{"type":"message","timestamp":"2026-08-02T09:00:00.998Z","message":{"role":"assistant","model":"grok-4.5","usage":{"input":100,"output":200,"cacheRead":10,"cacheWrite":5,"cost":{"total":0.05}}}}"#;
    let ev = extract_pi(line, Some("session-a"), None).unwrap().unwrap();
    assert_eq!(ev.input_tokens, 100);
    assert_eq!(ev.output_tokens, 200);
    assert_eq!(ev.cache_read_tokens, 10);
    assert_eq!(ev.cache_creation_tokens, 5);
    assert_eq!(ev.model, "grok-4.5");
    assert_eq!(ev.cost_usd, Some(0.05));
}

#[test]
fn pi_uses_model_hint_when_message_lacks_model() {
    let line = r#"{"type":"message","timestamp":"2026-01-02T00:00:00.000Z","message":{"role":"assistant","usage":{"input":1,"output":2}}}"#;
    let bare = extract_pi(line, None, None).unwrap().unwrap();
    assert_eq!(bare.model, "unknown");
    let hinted = extract_pi(line, None, Some("grok-4.5")).unwrap().unwrap();
    assert_eq!(hinted.model, "grok-4.5");
}

#[test]
fn pi_total_tokens_fallback() {
    let line = r#"{"type":"message","timestamp":"2026-01-02T00:00:00.000Z","message":{"role":"assistant","model":"grok-4.5","usage":{"totalTokens":333}}}"#;
    let ev = extract_pi(line, None, None).unwrap().unwrap();
    assert_eq!(ev.output_tokens, 333);
}

#[test]
fn pi_skips_non_assistant() {
    let line = r#"{"type":"message","timestamp":"2026-01-02T00:00:00.000Z","message":{"role":"user","usage":{"input":1}}}"#;
    assert!(extract_pi(line, None, None).unwrap().is_none());
}

#[test]
fn workbuddy_uses_claude_shape() {
    let line = r#"{"timestamp":"2026-01-09T10:00:00.000Z","sessionId":"wb1","message":{"id":"m1","model":"claude-sonnet-4","usage":{"input_tokens":10,"output_tokens":5,"cache_read_input_tokens":1}}}"#;
    let ev = extract_claude_like(AgentId::WorkBuddy, line, None)
        .unwrap()
        .unwrap();
    assert_eq!(ev.input_tokens, 10);
    assert_eq!(ev.agent_id, AgentId::WorkBuddy);
}

#[test]
fn is_kimi_wire_layout_accepts_old_and_new() {
    let sessions = PathBuf::from("/home/u/.kimi/sessions");
    let old = sessions.join("group").join("session-a").join("wire.jsonl");
    let new = sessions
        .join("ws")
        .join("session-b")
        .join("agents")
        .join("agent-1")
        .join("wire.jsonl");
    let bad = sessions
        .join("nested")
        .join("path")
        .join("session")
        .join("wire.jsonl");
    assert!(is_kimi_wire_layout(&sessions, &old));
    assert!(is_kimi_wire_layout(&sessions, &new));
    assert!(!is_kimi_wire_layout(&sessions, &bad));
}

#[test]
fn incremental_cursor_advances() {
    let dir = tempdir().unwrap();
    let db = Database::open(&dir.path().join("t.db")).unwrap();
    let repo = UsageRepo::new(db);
    let log = dir.path().join("a.jsonl");
    fs::write(
        &log,
        r#"{"message":{"model":"m","usage":{"input_tokens":1,"output_tokens":2}}}
"#,
    )
    .unwrap();
    let batch = parse_file_for_agent_id(AgentId::Claude, &log, &repo).unwrap();
    assert_eq!(batch.events.len(), 1);
    assert!(repo.get_cursor(&log.to_string_lossy()).unwrap().is_none());
    repo.insert_batch_and_cursors(&[], std::slice::from_ref(&batch.cursor))
        .unwrap();
    let cur = repo.get_cursor(&log.to_string_lossy()).unwrap().unwrap();
    assert!(cur.byte_offset > 0);

    let batch2 = parse_file_for_agent_id(AgentId::Claude, &log, &repo).unwrap();
    assert!(batch2.events.is_empty());
}

/// Smoke against real ~/.kimi-code when present (this developer machine).
#[test]
fn live_kimi_code_discover_and_parse_sample() {
    let Ok(home) = crate::utils::paths::home_dir() else {
        return;
    };
    let root = home.join(".kimi-code");
    if !root.join("sessions").is_dir() {
        return;
    }
    // config.toml default_model
    let dm = read_kimi_default_model(&root);
    assert!(dm.is_some(), "expected default_model from real config.toml");
    assert_ne!(dm.as_deref(), Some("kimi-for-coding")); // real machine uses k3

    let files = discover_kimi_wire_files().expect("discover");
    assert!(
        !files.is_empty(),
        "expected wire.jsonl under ~/.kimi-code/sessions"
    );

    // Parse the first non-empty wire file that actually has usage.record turns.
    let dir = tempdir().unwrap();
    let db = Database::open(&dir.path().join("t.db")).unwrap();
    let repo = UsageRepo::new(db);
    let mut sample = None;
    let mut batch = None;
    for path in files
        .iter()
        .filter(|p| fs::metadata(p).map(|m| m.len() > 1000).unwrap_or(false))
    {
        let parsed = parse_file_for_agent_id(AgentId::Kimi, path, &repo).expect("parse");
        if !parsed.events.is_empty() {
            sample = Some(path);
            batch = Some(parsed);
            break;
        }
    }
    let Some(batch) = batch else {
        return;
    };
    let _sample = sample.expect("usage sample path");
    // Models must come from wire lines / config — never invent product defaults
    assert!(
        batch.events.iter().all(|e| e.model != "unknown"),
        "unexpected unknown model when logs carry model fields"
    );
    let models: std::collections::BTreeSet<_> =
        batch.events.iter().map(|e| e.model.as_str()).collect();
    if !models.iter().any(|m| *m == "k3" || m.contains("kimi")) {
        // Optional live smoke for a Kimi-product ~/.kimi-code. Skip when this
        // machine's sessions are a custom/OpenAI-compatible relay (e.g. grok).
        return;
    }
}

/// Live ~/.codex: config model + real rollout token_count inheritance.
#[test]
fn live_codex_config_and_session_models() {
    let Ok(home) = crate::utils::paths::home_dir() else {
        return;
    };
    let root = home.join(".codex");
    if !root.join("sessions").is_dir() {
        return;
    }
    let Some(dm) = read_codex_default_model(&root) else {
        return;
    };
    assert_ne!(dm.as_str(), "gpt-5", "must not invent legacy gpt-5");

    let files = discover_usage_files(AgentId::Codex).expect("discover");
    if files.is_empty() {
        return;
    }
    let sample = files
        .iter()
        .rev()
        .find(|p| fs::metadata(p).map(|m| m.len() > 2000).unwrap_or(false))
        .expect("non-empty codex rollout");
    let dir = tempdir().unwrap();
    let db = Database::open(&dir.path().join("t.db")).unwrap();
    let repo = UsageRepo::new(db);
    let batch = parse_file_for_agent_id(AgentId::Codex, sample, &repo).expect("parse");
    if batch.events.is_empty() {
        return; // short session without token_count
    }
    assert!(
        batch
            .events
            .iter()
            .all(|e| e.model != "gpt-5" || dm == "gpt-5"),
        "must not invent gpt-5 unless config really says so; got {:?}",
        batch
            .events
            .iter()
            .map(|e| e.model.as_str())
            .collect::<Vec<_>>()
    );
    // Prefer real models observed on this machine (turn_context / config)
    let models: std::collections::BTreeSet<_> =
        batch.events.iter().map(|e| e.model.as_str()).collect();
    assert!(
        models.iter().all(|m| *m != "unknown"),
        "unexpected unknown when config/logs available: {models:?} cfg={dm}"
    );
}

/// Live ~/.claude/projects: models come from log lines, never hardcoded.
#[test]
fn live_claude_projects_models_from_logs() {
    let Ok(home) = crate::utils::paths::home_dir() else {
        return;
    };
    if !home.join(".claude").join("projects").is_dir() {
        return;
    }
    let files = discover_usage_files(AgentId::Claude).expect("discover");
    if files.is_empty() {
        return;
    }
    let sample = files
        .iter()
        .rev()
        .find(|p| fs::metadata(p).map(|m| m.len() > 5000).unwrap_or(false));
    let Some(sample) = sample else {
        return;
    };
    let dir = tempdir().unwrap();
    let db = Database::open(&dir.path().join("t.db")).unwrap();
    let repo = UsageRepo::new(db);
    let batch = parse_file_for_agent_id(AgentId::Claude, sample, &repo).expect("parse");
    if batch.events.is_empty() {
        return;
    }
    let models: std::collections::BTreeSet<_> =
        batch.events.iter().map(|e| e.model.as_str()).collect();
    // Live Claude project logs may contain third-party models (custom
    // providers). Only assert parser hygiene: no synthetic placeholders and
    // at least one non-empty model id.
    assert!(
        models.iter().all(|m| *m != "<synthetic>"),
        "synthetic models should be filtered: {models:?}"
    );
    assert!(
        models.iter().any(|m| !m.trim().is_empty()),
        "live claude parse produced empty model ids: {models:?}"
    );
}

/// Live ~/.pi/agent: settings defaultModel + session usage.
#[test]
fn live_pi_settings_and_session() {
    let Ok(home) = crate::utils::paths::home_dir() else {
        return;
    };
    let agent_dir = home.join(".pi").join("agent");
    if !agent_dir.is_dir() {
        return;
    }
    let dm = read_pi_default_model(&agent_dir);
    // Live settings may or may not pin a default model; the reader must
    // follow the file instead of assuming this machine's content.
    let pins_model = fs::read_to_string(agent_dir.join("settings.json"))
        .map(|text| {
            serde_json::from_str::<serde_json::Value>(&text)
                .ok()
                .map(|v| {
                    v.get("defaultModel")
                        .or_else(|| v.get("default_model"))
                        .and_then(|x| x.as_str())
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .is_some()
                })
                .unwrap_or(false)
        })
        .unwrap_or(false);
    assert_eq!(
        dm.is_some(),
        pins_model,
        "read_pi_default_model must follow live settings.json"
    );
    let files = discover_usage_files(AgentId::Pi).expect("discover");
    if files.is_empty() {
        return;
    }
    let sample = &files[0];
    let dir = tempdir().unwrap();
    let db = Database::open(&dir.path().join("t.db")).unwrap();
    let repo = UsageRepo::new(db);
    let batch = parse_file_for_agent_id(AgentId::Pi, sample, &repo).expect("parse");
    if batch.events.is_empty() {
        return;
    }
    assert!(
        batch.events.iter().all(|e| e.model != "unknown"),
        "pi messages carry model or settings hint"
    );
}

#[test]
fn extract_dsh_reads_real_rows_and_skips_heuristics() {
    // Shapes copied from a real `session.v3.jsonl.zstd` transcript: provider
    // usage lives in `data.usage`, the model in `data.message.source.model`.
    let header = r#"{"type":"request/header","seq":12,"time":1789102623320,"data":{"header":{"config":{"provider":"deepseek-official","model":"deepseek-flash","maxTokens":256000}}}}"#;
    let billed = r#"{"type":"assistant/message","seq":17,"time":1789102624657,"data":{"turn":1,"step":1,"message":{"role":"assistant","content":[{"type":"text","text":"好"}],"source":{"kind":"model","provider":"deepseek-official","model":"deepseek-flash"}},"usage":{"inputTokens":12,"outputTokens":34,"totalTokens":46,"cacheReadTokens":2,"reasoningTokens":7}},"surfaceOp":"append"}"#;
    // Defensive shapes: meter rows and seeded/inherited rows are billed at most
    // once. v3 session logs on disk only carry `isSeeded` in the header plus a
    // `session/end-seed` marker, so these guard against a future shape.
    let estimated = r#"{"type":"token-meter","seq":20,"time":1789102625000,"data":{"surfaceTokens":999,"estimated":true}}"#;
    let meter = r#"{"type":"token-meter","seq":21,"time":1789102625000,"data":{"surfaceTokens":999}}"#;
    let seed = r#"{"type":"session/end-seed","seq":31,"time":1789102627000,"data":{"seedLength":2,"usage":{"inputTokens":100,"outputTokens":100}}}"#;
    // Message content may quote our own vocabulary; that must not hide usage.
    let quotes_surface_tokens = r#"{"type":"assistant/message","seq":40,"time":1789102628000,"data":{"message":{"role":"assistant","content":[{"type":"text","text":"projcache has surfaceTokens and estimated:true fields"}],"source":{"kind":"model","model":"deepseek-flash"}},"usage":{"inputTokens":7,"outputTokens":8}}}"#;
    let mut model = None;
    note_dsh_model_from_line(header, &mut model);
    assert_eq!(model.as_deref(), Some("deepseek-flash"));

    let ev = extract_dsh(billed, Some("session-abc"), model.as_deref())
        .unwrap()
        .expect("provider usage");
    assert_eq!(ev.agent_id, AgentId::Dsh);
    assert_eq!(ev.model, "deepseek-flash");
    assert_eq!(ev.input_tokens, 12);
    assert_eq!(ev.output_tokens, 34);
    assert_eq!(ev.cache_read_tokens, 2);
    // `time` is epoch milliseconds, not seconds.
    assert_eq!(ev.ts, "2026-09-11T04:57:04.657+00:00");
    assert_eq!(ev.session_id.as_deref(), Some("session-abc"));

    assert!(line_might_have_usage_dsh(billed));
    assert!(line_might_have_usage_dsh(quotes_surface_tokens));
    let quoted = extract_dsh(quotes_surface_tokens, None, None)
        .unwrap()
        .expect("content quoting token words still bills");
    assert_eq!(quoted.input_tokens, 7);

    assert!(extract_dsh(meter, None, None).unwrap().is_none());
    assert!(extract_dsh(estimated, None, None).unwrap().is_none());
    assert!(extract_dsh(seed, None, None).unwrap().is_none());
}

#[test]
fn dsh_session_id_comes_from_the_session_directory() {
    let path = std::path::Path::new(
        "/home/box/.dsh/sessions/--D-work--/session-8ea28612/session.v3.jsonl.zstd",
    );
    assert_eq!(
        session_id_from_path(path).as_deref(),
        Some("session-8ea28612")
    );
    // A flat file has no session directory: do not invent an id from the stem.
    let flat = std::path::Path::new("/home/box/.dsh/sessions/session.v3.jsonl.zstd");
    assert_eq!(session_id_from_path(flat), None);
}

#[test]
fn parse_dsh_fixture_collects_two_events() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("session.v3.jsonl");
    std::fs::write(path.clone(), include_str!("../fixtures/dsh_session.jsonl")).unwrap();
    let db = Database::open(&dir.path().join("t.db")).unwrap();
    let repo = UsageRepo::new(db);
    let batch = parse_file_for_agent_id(AgentId::Dsh, &path, &repo).expect("parse");
    assert_eq!(batch.events.len(), 2);
    assert!(batch.events.iter().all(|e| e.agent_id == AgentId::Dsh));
    assert_eq!(batch.events[0].model, "deepseek-flash");
    assert_eq!(batch.events[0].cache_read_tokens, 2);
    assert_eq!(batch.events[1].output_tokens, 5);
    assert_eq!(batch.events[1].cost_usd, Some(0.001));
    assert!(batch.events.iter().all(|e| e.input_tokens < 50));
    assert!(batch.events.iter().all(|e| e.ts.starts_with("2026-")));
}

#[test]
fn parse_dsh_compressed_log_collects_events() {
    let dir = tempdir().unwrap();
    let path = dir
        .path()
        .join("sessions/--D-work--/session-abc/session.v3.jsonl.zstd");
    // One zstd frame per row, the way DSH appends batches.
    crate::utils::dsh_log_fixture::write_zstd_log(
        &path,
        &[
            r#"{"type":"session","version":3,"id":"session-abc","createdAt":1789102558446,"cwd":"/tmp/dsh"}"#,
            r#"{"type":"request/header","seq":12,"time":1789102623320,"data":{"header":{"config":{"model":"deepseek-flash"}}}}"#,
            r#"{"type":"assistant/message","seq":17,"time":1789102624657,"data":{"turn":1,"step":1,"message":{"source":{"kind":"model","model":"deepseek-flash"}},"usage":{"inputTokens":12,"outputTokens":34,"cacheReadTokens":2}}}"#,
        ],
    );
    let db = Database::open(&dir.path().join("t.db")).unwrap();
    let repo = UsageRepo::new(db);
    let batch = parse_file_for_agent_id(AgentId::Dsh, &path, &repo).expect("parse");
    assert_eq!(
        batch.events.len(),
        1,
        "compressed log must decode: {:?}",
        batch.events
    );
    let ev = &batch.events[0];
    assert_eq!(ev.model, "deepseek-flash");
    assert_eq!(ev.input_tokens, 12);
    assert_eq!(ev.cache_read_tokens, 2);
    assert_eq!(ev.session_id.as_deref(), Some("session-abc"));
}

#[test]
fn discover_dsh_files_only_known_roots() {
    use std::sync::Mutex;
    static ENV_LOCK: Mutex<()> = Mutex::new(());
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempdir().unwrap();
    let home = dir.path().join("dsh-home");
    let extra = dir.path().join("session-root");
    let cwd_sessions = dir.path().join(".sessions");
    std::fs::create_dir_all(home.join("sessions/node_modules")).unwrap();
    std::fs::create_dir_all(home.join("sessions/cache.db")).unwrap();
    std::fs::create_dir_all(home.join("profiles/headless/sessions")).unwrap();
    std::fs::create_dir_all(&extra).unwrap();
    std::fs::create_dir_all(&cwd_sessions).unwrap();
    std::fs::write(home.join("sessions/keep.jsonl"), "{}\n").unwrap();
    std::fs::write(
        home.join("profiles/headless/sessions/profile.jsonl"),
        "{}\n",
    )
    .unwrap();
    // DSH compresses by default; discovery must see these too.
    let zstd_log = home.join("sessions/--D-work--/session-abc/session.v3.jsonl.zstd");
    crate::utils::dsh_log_fixture::write_zstd_log(&zstd_log, &["{}"]);
    std::fs::write(home.join("sessions/node_modules/skip.jsonl"), "{}\n").unwrap();
    std::fs::write(home.join("sessions/cache.db/skip.jsonl"), "{}\n").unwrap();
    std::fs::write(extra.join("extra.jsonl"), "{}\n").unwrap();
    std::fs::write(cwd_sessions.join("random.jsonl"), "{}\n").unwrap();

    let prev_home = std::env::var_os("DSH_HOME");
    let prev_root = std::env::var_os("DSH_SESSION_ROOT");
    std::env::set_var("DSH_HOME", &home);
    std::env::set_var("DSH_SESSION_ROOT", &extra);
    let files = discover_dsh_files().expect("discover");
    match prev_home {
        Some(v) => std::env::set_var("DSH_HOME", v),
        None => std::env::remove_var("DSH_HOME"),
    }
    match prev_root {
        Some(v) => std::env::set_var("DSH_SESSION_ROOT", v),
        None => std::env::remove_var("DSH_SESSION_ROOT"),
    }

    let names: Vec<String> = files
        .iter()
        .filter_map(|p| p.file_name().and_then(|n| n.to_str()).map(str::to_string))
        .collect();
    assert!(names.contains(&"keep.jsonl".into()), "{names:?}");
    assert!(names.contains(&"profile.jsonl".into()), "{names:?}");
    assert!(names.contains(&"extra.jsonl".into()), "{names:?}");
    assert!(
        names.contains(&"session.v3.jsonl.zstd".into()),
        "compressed DSH logs are discoverable: {names:?}"
    );
    assert!(!names.contains(&"skip.jsonl".into()), "{names:?}");
    assert!(!names.contains(&"random.jsonl".into()), "{names:?}");
    assert!(files.iter().all(|p| {
        let s = p.to_string_lossy();
        !s.contains("node_modules") && !s.contains(".db") && !s.contains(".sessions")
    }));
}

#[test]
fn claude_cache_creation_1h_stays_split() {
    let line = r#"{"timestamp":"2026-01-09T10:00:00.000Z","message":{"role":"assistant","id":"m1","model":"claude-sonnet-4-5","usage":{"input_tokens":10,"output_tokens":1,"cache_creation_input_tokens":300,"cache_creation":{"ephemeral_5m_input_tokens":100,"ephemeral_1h_input_tokens":200},"cache_read_input_tokens":4}},"requestId":"r1"}"#;
    let ev = extract_claude_like(AgentId::Claude, line, Some("s"))
        .unwrap()
        .unwrap();
    assert_eq!(ev.cache_creation_tokens, 100);
    assert_eq!(ev.cache_creation_1h_tokens, 200);
    assert_eq!(ev.cache_read_tokens, 4);
    assert_eq!(ev.cache_tokens_total(), 304);
}

#[test]
fn codex_fast_tier_from_thread_settings() {
    let settings = r#"{"timestamp":"2026-07-09T08:00:00.000Z","type":"event_msg","payload":{"type":"thread_settings_applied","thread_settings":{"service_tier":"priority"}}}"#;
    let tok = r#"{"timestamp":"2026-07-09T08:01:00.000Z","type":"event_msg","payload":{"type":"token_count","info":{"model":"gpt-5.6-sol","last_token_usage":{"input_tokens":100,"cached_input_tokens":0,"output_tokens":1,"total_tokens":101}}}}"#;
    let mut state = CodexParseState::default();
    assert!(extract_codex(settings, Some("s"), &mut state)
        .unwrap()
        .is_none());
    let ev = extract_codex(tok, Some("s"), &mut state).unwrap().unwrap();
    assert!(ev.fast);
    assert_eq!(ev.input_tokens, 100);
}

#[test]
fn discover_codex_prefers_live_sessions_over_archived_same_path() {
    let dir = tempdir().unwrap();
    let sessions = dir.path().join("sessions");
    let archived = dir.path().join("archived_sessions");
    fs::create_dir_all(sessions.join("dup")).unwrap();
    fs::create_dir_all(archived.join("dup")).unwrap();
    fs::create_dir_all(archived.join("only")).unwrap();
    fs::write(sessions.join("dup/rollout.jsonl"), "{}\n").unwrap();
    fs::write(archived.join("dup/rollout.jsonl"), "{archived}\n").unwrap();
    fs::write(archived.join("only/rollout.jsonl"), "{only}\n").unwrap();
    let files = discover_codex_files_in(dir.path());
    let names: Vec<_> = files
        .iter()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .collect();
    assert!(
        names
            .iter()
            .any(|n| n.contains("/sessions/dup/rollout.jsonl")),
        "{names:?}"
    );
    assert!(
        names
            .iter()
            .any(|n| n.contains("/archived_sessions/only/rollout.jsonl")),
        "{names:?}"
    );
    assert!(
        !names
            .iter()
            .any(|n| n.contains("/archived_sessions/dup/rollout.jsonl")),
        "archived duplicate should yield to live sessions: {names:?}"
    );
}

#[test]
fn read_codex_fast_service_tier_from_toml() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("config.toml"), "service_tier = \"fast\"\n").unwrap();
    assert!(read_codex_fast_service_tier(dir.path()));
    fs::write(
        dir.path().join("config.toml"),
        "service_tier = \"standard\"\n",
    )
    .unwrap();
    assert!(!read_codex_fast_service_tier(dir.path()));
    fs::write(
        dir.path().join("config.toml"),
        "[profiles.work]\nservice_tier = \"priority\"\n",
    )
    .unwrap();
    assert!(
        !read_codex_fast_service_tier(dir.path()),
        "profile-only Fast must not mark every session Fast"
    );
}
