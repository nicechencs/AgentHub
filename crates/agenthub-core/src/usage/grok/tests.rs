use std::fs;
use std::path::PathBuf;

use tempfile::tempdir;

use super::*;
use crate::platform::usage::parse_file_for_agent_id;
use crate::storage::{Database, UsageRepo};

fn sample_turn_completed_line() -> String {
    r#"{"timestamp":1750000000,"method":"_x.ai/session/update","params":{"sessionId":"sess-1","update":{"sessionUpdate":"turn_completed","usage":{"inputTokens":100,"outputTokens":20,"cachedReadTokens":40,"reasoningTokens":10,"totalTokens":120,"modelUsage":{"grok-4.5-build":{"inputTokens":100,"outputTokens":20,"cachedReadTokens":40,"reasoningTokens":10,"totalTokens":120}}}},"_meta":{"eventId":"evt-1"}}}"#.to_string()
}

fn turn_line(
    event_id: &str,
    model: &str,
    input: i64,
    output: i64,
    cache: i64,
    reasoning: i64,
    envelope_seconds: i64,
    agent_ms: Option<i64>,
) -> String {
    let mut meta = serde_json::json!({ "eventId": event_id });
    if let Some(ms) = agent_ms {
        meta["agentTimestampMs"] = serde_json::json!(ms);
    }
    serde_json::json!({
        "timestamp": envelope_seconds,
        "params": {
            "sessionId": "sess-1",
            "update": {
                "sessionUpdate": "turn_completed",
                "usage": {
                    "inputTokens": input,
                    "outputTokens": output,
                    "cachedReadTokens": cache,
                    "reasoningTokens": reasoning,
                    "modelUsage": {
                        model: {
                            "inputTokens": input,
                            "outputTokens": output,
                            "cachedReadTokens": cache,
                            "reasoningTokens": reasoning,
                        }
                    }
                }
            },
            "_meta": meta
        }
    })
    .to_string()
}

fn parse_line(line: &str, summary_model: Option<&str>) -> Vec<ParsedUsageEvent> {
    extract_grok_events(line, Some("path-sess"), None, summary_model).unwrap()
}

#[test]
fn splits_uncached_input_from_cache() {
    assert_eq!(split_input_tokens(100, 40, 0), (60, 40, 0));
    assert_eq!(split_input_tokens(10, 40, 0), (0, 10, 0));
    assert_eq!(split_input_tokens(0, 5, 0), (0, 0, 0));
}

#[test]
fn splits_cache_creation_out_of_the_uncached_input() {
    assert_eq!(split_input_tokens(100, 40, 25), (35, 40, 25));
    assert_eq!(split_input_tokens(100, 40, 999), (0, 40, 60));
}

#[test]
fn pricing_candidates_strip_build_and_add_xai() {
    assert_eq!(
        pricing_candidates("grok-4.5-build"),
        vec![
            "grok-4.5-build".to_string(),
            "xai/grok-4.5-build".to_string(),
            "x-ai/grok-4.5-build".to_string(),
            "grok-4.5".to_string(),
            "xai/grok-4.5".to_string(),
            "x-ai/grok-4.5".to_string(),
        ]
    );
}

#[test]
fn pricing_candidates_strip_grok_bracket_prefix() {
    assert_eq!(
        pricing_candidates("[grok] grok-4.5-build")[0],
        "grok-4.5-build"
    );
    assert!(pricing_candidates("   ").is_empty());
    assert!(pricing_candidates("[grok] ").is_empty());
}

#[test]
fn grok_4_5_build_resolves_embedded_pricing() {
    assert!(grok_model_has_pricing("grok-4.5-build"));
    assert!(!grok_model_has_pricing("grok-never-priced-build"));
}

#[test]
fn turn_completed_model_usage_maps_tokens_without_double_count() {
    let events = parse_line(&sample_turn_completed_line(), Some("grok-4.5"));
    assert_eq!(events.len(), 1);
    let ev = &events[0];
    assert_eq!(ev.model, "grok-4.5-build");
    assert_eq!(ev.input_tokens, 60);
    assert_eq!(ev.cache_read_tokens, 40);
    assert_eq!(ev.cache_creation_tokens, 0);
    assert_eq!(ev.output_tokens, 20);
    assert!(ev.cost_usd.is_none());
    assert_eq!(ev.session_id.as_deref(), Some("sess-1"));
    assert_eq!(ev.agent_id, AgentId::Grok);
}

#[test]
fn does_not_add_reasoning_tokens_to_stored_output() {
    let line = turn_line("evt-r", "grok-4.5-build", 0, 0, 0, 42, 1_750_000_000, None);
    let events = parse_line(&line, None);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].input_tokens, 0);
    assert_eq!(events[0].output_tokens, 0);
    assert_eq!(events[0].cache_read_tokens, 0);
}

#[test]
fn falls_back_to_top_level_usage_when_model_usage_is_absent() {
    let line = r#"{"timestamp":1750000000,"params":{"sessionId":"sess-top","update":{"sessionUpdate":"turn_completed","usage":{"inputTokens":50,"outputTokens":5,"cachedReadTokens":10,"reasoningTokens":2}},"_meta":{"eventId":"evt-top"}}}"#;
    let events = extract_grok_events(line, None, None, Some("grok-4.5-build")).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].model, "grok-4.5-build");
    assert_eq!(events[0].input_tokens, 40);
    assert_eq!(events[0].cache_read_tokens, 10);
    assert_eq!(events[0].output_tokens, 5);
}

#[test]
fn names_top_level_usage_unknown_when_no_default_model() {
    let line = r#"{"timestamp":1750000000,"params":{"sessionId":"sess-u","update":{"sessionUpdate":"turn_completed","usage":{"inputTokens":10,"outputTokens":1,"cachedReadTokens":0,"reasoningTokens":0}},"_meta":{"eventId":"evt-u"}}}"#;
    let events = parse_line(line, None);
    assert_eq!(events[0].model, "unknown");
}

#[test]
fn prefers_agent_timestamp_ms_over_envelope_seconds() {
    let line = turn_line(
        "evt-ts",
        "grok-4.5-build",
        10,
        1,
        0,
        0,
        1_750_000_000,
        Some(1_785_328_986_355),
    );
    let events = parse_line(&line, None);
    assert_eq!(events.len(), 1);
    assert!(events[0].ts.starts_with("2026-07-29"));
}

#[test]
fn converts_envelope_unix_seconds_to_rfc3339() {
    let line = turn_line(
        "evt-sec",
        "grok-4.5-build",
        10,
        1,
        0,
        0,
        1_750_000_000,
        None,
    );
    let events = parse_line(&line, None);
    assert_eq!(events.len(), 1);
    assert!(events[0].ts.starts_with("2025-06-15"));
}

#[test]
fn reads_the_recorded_cost_usd_ticks() {
    let line = r#"{"timestamp":1750000000,"params":{"sessionId":"sess-1","update":{"sessionUpdate":"turn_completed","usage":{"inputTokens":18444,"outputTokens":130,"cachedReadTokens":11264,"reasoningTokens":73,"costUsdTicks":185192000,"modelUsage":{"grok-4.5-build":{"inputTokens":18444,"outputTokens":130,"cachedReadTokens":11264,"reasoningTokens":73,"costUsdTicks":185192000}}}},"_meta":{"eventId":"evt-cost"}}}"#;
    let events = parse_line(line, None);
    assert_eq!(events.len(), 1);
    let cost = events[0].cost_usd.expect("ticks");
    assert!((cost - 0.0185192).abs() < 1e-12, "got {cost}");
    assert_eq!(events[0].input_tokens, 7180);
    assert_eq!(events[0].cache_read_tokens, 11264);
}

#[test]
fn reads_cache_creation_tokens() {
    let line = r#"{"timestamp":1750000000,"params":{"sessionId":"sess-1","update":{"sessionUpdate":"turn_completed","usage":{"inputTokens":100,"outputTokens":20,"cachedReadTokens":40,"cacheCreationTokens":25,"reasoningTokens":10,"modelUsage":{"grok-4.5-build":{"inputTokens":100,"outputTokens":20,"cachedReadTokens":40,"cacheCreationTokens":25,"reasoningTokens":10}}}},"_meta":{"eventId":"evt-cc"}}}"#;
    let events = parse_line(line, None);
    assert_eq!(events[0].input_tokens, 35);
    assert_eq!(events[0].cache_read_tokens, 40);
    assert_eq!(events[0].cache_creation_tokens, 25);
    assert_eq!(events[0].output_tokens, 20);
}

#[test]
fn skips_turn_completed_without_usage_and_zero_rows() {
    let skip = [
        r#"{"timestamp":1750000001,"params":{"update":{"sessionUpdate":"tool_call"}}}"#,
        r#"{"timestamp":1750000002,"params":{"update":{"sessionUpdate":"turn_completed"},"_meta":{"eventId":"no-usage"}}}"#,
        r#"{"timestamp":1750000003,"params":{"update":{"sessionUpdate":"turn_completed","usage":{"inputTokens":0,"outputTokens":0,"cachedReadTokens":0,"reasoningTokens":0,"modelUsage":{"grok-4.5":{"inputTokens":0,"outputTokens":0,"cachedReadTokens":0,"reasoningTokens":0}}}},"_meta":{"eventId":"zero"}}}"#,
    ];
    for line in skip {
        assert!(parse_line(line, None).is_empty(), "should skip {line}");
    }
    assert_eq!(parse_line(&sample_turn_completed_line(), None).len(), 1);
}

#[test]
fn multi_model_turn_emits_one_event_per_model() {
    let line = r#"{"timestamp":1750000100,"params":{"sessionId":"sess-m","update":{"sessionUpdate":"turn_completed","usage":{"modelUsage":{"model-a":{"inputTokens":10,"outputTokens":2,"cachedReadTokens":0,"reasoningTokens":1},"model-b":{"inputTokens":20,"outputTokens":4,"cachedReadTokens":5,"reasoningTokens":0}}}},"_meta":{"eventId":"evt-multi"}}}"#;
    let mut events = parse_line(line, None);
    events.sort_by(|a, b| a.model.cmp(&b.model));
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].model, "model-a");
    assert_eq!(events[0].input_tokens, 10);
    assert_eq!(events[1].model, "model-b");
    assert_eq!(events[1].input_tokens, 15);
    assert_eq!(events[1].cache_read_tokens, 5);
}

#[test]
fn parser_dedupes_same_event_id_and_model() {
    let line = sample_turn_completed_line();
    let dir = tempdir().unwrap();
    let path = dir.path().join("updates.jsonl");
    fs::write(&path, format!("{line}\n{line}\n")).unwrap();
    let mut parser = GrokParser::new(&path);
    let mut all = Vec::new();
    for raw in fs::read_to_string(&path).unwrap().lines() {
        all.extend(parser.extract_line(raw, Some("sess-1")).unwrap());
    }
    assert_eq!(all.len(), 1);
}

#[test]
fn parser_keeps_distinct_models_that_share_an_event_id() {
    let line = r#"{"timestamp":1750000100,"params":{"sessionId":"sess-m","update":{"sessionUpdate":"turn_completed","usage":{"modelUsage":{"model-a":{"inputTokens":10,"outputTokens":1,"cachedReadTokens":0,"reasoningTokens":0},"model-b":{"inputTokens":20,"outputTokens":2,"cachedReadTokens":0,"reasoningTokens":0}}}},"_meta":{"eventId":"evt-shared"}}}"#;
    let events = parse_line(line, None);
    assert_eq!(events.len(), 2);
}

#[test]
fn summary_id_fills_session_when_line_omits_it() {
    let line = r#"{"timestamp":1750000000,"params":{"update":{"sessionUpdate":"turn_completed","usage":{"modelUsage":{"grok-4.5-build":{"inputTokens":10,"outputTokens":1,"cachedReadTokens":0,"reasoningTokens":0}}}},"_meta":{"eventId":"evt-meta"}}}"#;
    let events =
        extract_grok_events(line, Some("path-sess"), Some("canonical-session"), None).unwrap();
    assert_eq!(events[0].session_id.as_deref(), Some("canonical-session"));
}

#[test]
fn line_session_id_beats_summary_id() {
    let events = extract_grok_events(
        &sample_turn_completed_line(),
        Some("path-sess"),
        Some("canonical-session"),
        None,
    )
    .unwrap();
    assert_eq!(events[0].session_id.as_deref(), Some("sess-1"));
}

#[test]
fn discover_only_updates_jsonl() {
    let dir = tempdir().unwrap();
    let sess = dir
        .path()
        .join("sessions")
        .join("proj")
        .join("019fa1b1-0000-7000-8000-000000000001");
    fs::create_dir_all(&sess).unwrap();
    fs::write(sess.join("updates.jsonl"), "{}\n").unwrap();
    fs::write(sess.join("events.jsonl"), "{}\n").unwrap();
    let logs = dir.path().join("logs");
    fs::create_dir_all(&logs).unwrap();
    fs::write(logs.join("unified.jsonl"), "{}\n").unwrap();

    let files = discover_grok_files_in(dir.path());
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0].file_name().and_then(|n| n.to_str()),
        Some("updates.jsonl")
    );
}

#[test]
fn session_id_from_updates_path_uses_parent_dir() {
    let path = PathBuf::from(
        r"C:\Users\u\.grok\sessions\C%3A%5Cproj\019fa1b1-0000-7000-8000-000000000001\updates.jsonl",
    );
    assert_eq!(
        session_id_from_updates_path(&path).as_deref(),
        Some("019fa1b1-0000-7000-8000-000000000001")
    );
    assert_eq!(
        crate::usage::session_jsonl::session_id_from_path(&path).as_deref(),
        Some("019fa1b1-0000-7000-8000-000000000001")
    );
}

#[test]
fn parse_file_through_usage_source_splits_tokens_and_reads_summary() {
    let dir = tempdir().unwrap();
    let sess = dir.path().join("sessions").join("proj").join("sess-file");
    fs::create_dir_all(&sess).unwrap();
    fs::write(
        sess.join("updates.jsonl"),
        sample_turn_completed_line() + "\n",
    )
    .unwrap();
    fs::write(
        sess.join("summary.json"),
        r#"{"info":{"id":"sess-file","cwd":"D:\\work\\proj"},"current_model_id":"grok-4.5"}"#,
    )
    .unwrap();

    let db = Database::open(&dir.path().join("t.db")).unwrap();
    let repo = UsageRepo::new(db);
    let batch = parse_file_for_agent_id(AgentId::Grok, &sess.join("updates.jsonl"), &repo).unwrap();
    assert_eq!(batch.events.len(), 1);
    assert_eq!(batch.events[0].model, "grok-4.5-build");
    assert_eq!(batch.events[0].input_tokens, 60);
    assert_eq!(batch.events[0].cache_read_tokens, 40);
    assert_eq!(batch.events[0].output_tokens, 20);
    assert_eq!(batch.events[0].session_id.as_deref(), Some("sess-1"));
}

#[test]
fn cost_usd_from_ticks_treats_zero_as_missing() {
    assert!(cost_usd_from_ticks(0).is_none());
    let cost = cost_usd_from_ticks(185_192_000).expect("ticks");
    assert!((cost - 0.0185192).abs() < 1e-12, "got {cost}");
}

#[test]
fn zero_cost_ticks_on_a_turn_do_not_store_zero_dollars() {
    let line = r#"{"timestamp":1750000000,"params":{"sessionId":"sess-1","update":{"sessionUpdate":"turn_completed","usage":{"modelUsage":{"grok-4.5-build":{"inputTokens":10,"outputTokens":1,"cachedReadTokens":0,"reasoningTokens":0,"costUsdTicks":0}}}},"_meta":{"eventId":"evt-z"}}}"#;
    let events = parse_line(line, None);
    assert_eq!(events.len(), 1);
    assert!(events[0].cost_usd.is_none());
}

#[test]
fn rejects_invalid_json() {
    assert!(extract_grok_events("{not json", None, None, None).is_err());
}

#[test]
fn uses_unix_epoch_when_no_timestamp_fields_are_present() {
    let line = r#"{"params":{"sessionId":"sess-e","update":{"sessionUpdate":"turn_completed","usage":{"modelUsage":{"grok-4.5-build":{"inputTokens":1,"outputTokens":1,"cachedReadTokens":0,"reasoningTokens":0}}}},"_meta":{"eventId":"evt-e"}}}"#;
    let events = parse_line(line, None);
    assert_eq!(events.len(), 1);
    assert!(
        events[0].ts.starts_with("1970-01-01"),
        "got {}",
        events[0].ts
    );
}

#[test]
fn accepts_envelope_timestamp_already_in_millis() {
    let line = r#"{"timestamp":1750000000000,"params":{"sessionId":"sess-1","update":{"sessionUpdate":"turn_completed","usage":{"modelUsage":{"grok-4.5-build":{"inputTokens":1,"outputTokens":1,"cachedReadTokens":0,"reasoningTokens":0}}}},"_meta":{"eventId":"evt-ms"}}}"#;
    let events = parse_line(line, None);
    assert_eq!(events.len(), 1);
    assert!(events[0].ts.starts_with("2025-06-15"));
}

#[test]
fn reads_snake_case_usage_fields() {
    let line = r#"{"timestamp":1750000000,"params":{"session_id":"sess-snake","update":{"sessionUpdate":"turn_completed","usage":{"input_tokens":100,"output_tokens":20,"cached_read_tokens":40,"cache_creation_tokens":25,"reasoning_tokens":10,"cost_usd_ticks":1000000000}},"_meta":{"eventId":"evt-snake"}}}"#;
    let events = extract_grok_events(line, None, None, Some("grok-4.5-build")).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].session_id.as_deref(), Some("sess-snake"));
    assert_eq!(events[0].input_tokens, 35);
    assert_eq!(events[0].cache_read_tokens, 40);
    assert_eq!(events[0].cache_creation_tokens, 25);
    assert_eq!(events[0].output_tokens, 20);
    let cost = events[0].cost_usd.expect("ticks");
    assert!((cost - 0.1).abs() < 1e-12, "got {cost}");
}

#[test]
fn truncates_float_token_counts() {
    let line = r#"{"timestamp":1750000000,"params":{"sessionId":"sess-f","update":{"sessionUpdate":"turn_completed","usage":{"modelUsage":{"grok-4.5-build":{"inputTokens":100.9,"outputTokens":20.2,"cachedReadTokens":40.1,"reasoningTokens":0}}}},"_meta":{"eventId":"evt-f"}}}"#;
    let events = parse_line(line, None);
    assert_eq!(events[0].input_tokens, 60);
    assert_eq!(events[0].cache_read_tokens, 40);
    assert_eq!(events[0].output_tokens, 20);
}

#[test]
fn path_session_id_used_when_line_and_summary_omit_it() {
    let line = r#"{"timestamp":1750000000,"params":{"update":{"sessionUpdate":"turn_completed","usage":{"modelUsage":{"grok-4.5-build":{"inputTokens":10,"outputTokens":1,"cachedReadTokens":0,"reasoningTokens":0}}}},"_meta":{"eventId":"evt-path"}}}"#;
    let events = extract_grok_events(line, Some("019fa1b1-path"), None, None).unwrap();
    assert_eq!(events[0].session_id.as_deref(), Some("019fa1b1-path"));
}

#[test]
fn parser_loads_summary_default_model_for_top_level_usage() {
    let line = r#"{"timestamp":1750000000,"params":{"sessionId":"sess-sum","update":{"sessionUpdate":"turn_completed","usage":{"inputTokens":50,"outputTokens":5,"cachedReadTokens":10,"reasoningTokens":0}},"_meta":{"eventId":"evt-sum"}}}"#;
    let dir = tempdir().unwrap();
    let path = dir.path().join("updates.jsonl");
    fs::write(&path, format!("{line}\n")).unwrap();
    fs::write(
        dir.path().join("summary.json"),
        r#"{"info":{"id":"canonical"},"current_model_id":"grok-4.5-build"}"#,
    )
    .unwrap();
    let mut parser = GrokParser::new(&path);
    let events = parser.extract_line(&line, Some("path-sess")).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].model, "grok-4.5-build");
}

#[test]
fn parser_dedupes_identical_rows_without_event_id() {
    let line = r#"{"timestamp":1750000000,"params":{"sessionId":"sess-d","update":{"sessionUpdate":"turn_completed","usage":{"modelUsage":{"grok-4.5-build":{"inputTokens":100,"outputTokens":20,"cachedReadTokens":40,"reasoningTokens":10}}}}}}"#;
    let dir = tempdir().unwrap();
    let path = dir.path().join("updates.jsonl");
    fs::write(&path, format!("{line}\n{line}\n")).unwrap();
    let mut parser = GrokParser::new(&path);
    let mut all = Vec::new();
    for raw in fs::read_to_string(&path).unwrap().lines() {
        all.extend(parser.extract_line(raw, Some("sess-d")).unwrap());
    }
    assert_eq!(all.len(), 1);
}

#[test]
fn parser_keeps_rows_without_event_id_that_differ_only_in_cache_creation() {
    let with_creation = r#"{"timestamp":1750000000,"params":{"sessionId":"sess-cc","update":{"sessionUpdate":"turn_completed","usage":{"modelUsage":{"grok-4.5-build":{"inputTokens":100,"outputTokens":5,"cachedReadTokens":0,"cacheCreationTokens":20,"reasoningTokens":0}}}}}}"#;
    let without_creation = r#"{"timestamp":1750000000,"params":{"sessionId":"sess-cc","update":{"sessionUpdate":"turn_completed","usage":{"modelUsage":{"grok-4.5-build":{"inputTokens":80,"outputTokens":5,"cachedReadTokens":0,"reasoningTokens":0}}}}}}"#;
    let dir = tempdir().unwrap();
    let path = dir.path().join("updates.jsonl");
    fs::write(&path, format!("{with_creation}\n{without_creation}\n")).unwrap();
    let mut parser = GrokParser::new(&path);
    let mut all = Vec::new();
    for raw in fs::read_to_string(&path).unwrap().lines() {
        all.extend(parser.extract_line(raw, Some("sess-cc")).unwrap());
    }
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].cache_creation_tokens, 20);
    assert_eq!(all[1].cache_creation_tokens, 0);
}

#[test]
fn line_might_have_usage_only_matches_turn_completed() {
    assert!(line_might_have_usage_grok(
        r#"{"sessionUpdate":"turn_completed"}"#
    ));
    assert!(!line_might_have_usage_grok(
        r#"{"sessionUpdate":"tool_call","usage":{}}"#
    ));
}

#[test]
fn discover_sorts_nested_session_trees() {
    let dir = tempdir().unwrap();
    let a = dir.path().join("sessions").join("proj-b").join("sess-2");
    let b = dir.path().join("sessions").join("proj-a").join("sess-1");
    fs::create_dir_all(&a).unwrap();
    fs::create_dir_all(&b).unwrap();
    fs::write(a.join("updates.jsonl"), "{}\n").unwrap();
    fs::write(b.join("updates.jsonl"), "{}\n").unwrap();
    let files = discover_grok_files_in(dir.path());
    assert_eq!(files.len(), 2);
    assert!(files[0] < files[1], "discovery must be path-sorted");
}

#[test]
fn discover_empty_home_is_empty() {
    let dir = tempdir().unwrap();
    assert!(discover_grok_files_in(dir.path()).is_empty());
}

#[test]
fn parse_file_counts_invalid_turn_completed_as_failed() {
    let dir = tempdir().unwrap();
    let sess = dir.path().join("sessions").join("proj").join("sess-bad");
    fs::create_dir_all(&sess).unwrap();
    fs::write(
        sess.join("updates.jsonl"),
        format!(
            "{{this is not json but has \"turn_completed\"\n{}\n",
            sample_turn_completed_line()
        ),
    )
    .unwrap();
    let db = Database::open(&dir.path().join("t.db")).unwrap();
    let repo = UsageRepo::new(db);
    let batch = parse_file_for_agent_id(AgentId::Grok, &sess.join("updates.jsonl"), &repo).unwrap();
    assert_eq!(batch.failed, 1);
    assert_eq!(batch.events.len(), 1);
}

#[test]
fn incremental_cursor_skips_unchanged_file() {
    let dir = tempdir().unwrap();
    let sess = dir.path().join("sessions").join("proj").join("sess-inc");
    fs::create_dir_all(&sess).unwrap();
    let path = sess.join("updates.jsonl");
    let first = turn_line("evt-a", "grok-4.5-build", 10, 1, 0, 0, 1_750_000_000, None);
    fs::write(&path, format!("{first}\n")).unwrap();

    let db = Database::open(&dir.path().join("t.db")).unwrap();
    let repo = UsageRepo::new(db);
    let batch1 = parse_file_for_agent_id(AgentId::Grok, &path, &repo).unwrap();
    assert_eq!(batch1.events.len(), 1);
    repo.insert_batch_and_cursors(&[], std::slice::from_ref(&batch1.cursor))
        .unwrap();

    let batch2 = parse_file_for_agent_id(AgentId::Grok, &path, &repo).unwrap();
    assert!(
        batch2.events.is_empty(),
        "unchanged mtime must resume from the stored byte offset"
    );
}

#[test]
fn real_cli_turn_completed_shape_splits_and_reads_ticks() {
    // Verbatim shape from Grok Build CLI 1.0.x (fields beyond usage are ignored).
    let line = r#"{"timestamp":1785509402,"method":"_x.ai/session/update","params":{"sessionId":"019fb8a7-06a3-7cb2-83e6-980123542122","update":{"sessionUpdate":"turn_completed","prompt_id":"611f0423-83f4-4af6-9a6e-637904f342d5","stop_reason":"cancelled","usage":{"inputTokens":49057,"outputTokens":987,"totalTokens":50044,"cachedReadTokens":21504,"cacheCreationTokens":0,"reasoningTokens":586,"modelCalls":2,"apiDurationMs":19968,"costUsdTicks":674792000,"modelUsage":{"grok-4.5-build":{"inputTokens":49057,"outputTokens":987,"totalTokens":50044,"cachedReadTokens":21504,"cacheCreationTokens":0,"reasoningTokens":586,"modelCalls":2,"apiDurationMs":19968,"costUsdTicks":674792000}},"numTurns":2}},"_meta":{"eventId":"019fb8a7-06a3-7cb2-83e6-980123542122-228","agentTimestampMs":1785509402900}}}"#;
    let events = parse_line(line, None);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].model, "grok-4.5-build");
    assert_eq!(events[0].input_tokens, 27553);
    assert_eq!(events[0].cache_read_tokens, 21504);
    assert_eq!(events[0].cache_creation_tokens, 0);
    assert_eq!(events[0].output_tokens, 987);
    assert_eq!(
        events[0].session_id.as_deref(),
        Some("019fb8a7-06a3-7cb2-83e6-980123542122")
    );
    assert_eq!(
        events[0].raw_hash,
        "019fb8a7-06a3-7cb2-83e6-980123542122-228|grok-4.5-build"
    );
    let cost = events[0].cost_usd.expect("ticks");
    assert!((cost - 0.0674792).abs() < 1e-12, "got {cost}");
    assert!(events[0].ts.starts_with("2026-07-31"));
}

#[test]
fn parse_file_emits_one_event_per_model_on_shared_turn() {
    let line = r#"{"timestamp":1750000100,"params":{"sessionId":"sess-m","update":{"sessionUpdate":"turn_completed","usage":{"modelUsage":{"model-a":{"inputTokens":10,"outputTokens":1,"cachedReadTokens":0,"reasoningTokens":0},"model-b":{"inputTokens":20,"outputTokens":2,"cachedReadTokens":5,"reasoningTokens":0}}}},"_meta":{"eventId":"evt-shared"}}}"#;
    let dir = tempdir().unwrap();
    let sess = dir.path().join("sessions").join("proj").join("sess-m");
    fs::create_dir_all(&sess).unwrap();
    fs::write(sess.join("updates.jsonl"), format!("{line}\n")).unwrap();

    let db = Database::open(&dir.path().join("t.db")).unwrap();
    let repo = UsageRepo::new(db);
    let mut events = parse_file_for_agent_id(AgentId::Grok, &sess.join("updates.jsonl"), &repo)
        .unwrap()
        .events;
    events.sort_by(|a, b| a.model.cmp(&b.model));
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].model, "model-a");
    assert_eq!(events[0].input_tokens, 10);
    assert_eq!(events[1].model, "model-b");
    assert_eq!(events[1].input_tokens, 15);
    assert_eq!(events[1].cache_read_tokens, 5);
}

#[test]
fn live_grok_home_parses_turn_completed_when_present() {
    let Ok(home) = crate::utils::paths::home_dir() else {
        return;
    };
    let root = home.join(".grok");
    if !root.join("sessions").is_dir() {
        return;
    }
    let files = discover_grok_files_in(&root);
    // Auth-error turns write `turn_completed` without `usage` — skip those files.
    let Some(sample) = files.iter().find(|p| {
        fs::read_to_string(p)
            .map(|text| text.contains("\"turn_completed\"") && text.contains("\"inputTokens\""))
            .unwrap_or(false)
    }) else {
        return;
    };
    let dir = tempdir().unwrap();
    let db = Database::open(&dir.path().join("t.db")).unwrap();
    let repo = UsageRepo::new(db);
    let batch = parse_file_for_agent_id(AgentId::Grok, sample, &repo).expect("parse");
    assert!(
        !batch.events.is_empty(),
        "usage-bearing turn_completed in {} produced no events",
        sample.display()
    );
    assert!(
        batch.events.iter().all(|e| e.agent_id == AgentId::Grok),
        "grok events must stay on the grok agent"
    );
    assert!(
        batch
            .events
            .iter()
            .any(|e| e.model.contains("grok") || e.model != "unknown"),
        "expected a real model id from live updates.jsonl, got {:?}",
        batch
            .events
            .iter()
            .map(|e| e.model.as_str())
            .collect::<Vec<_>>()
    );
    assert!(
        batch
            .events
            .iter()
            .all(|e| e.session_id.as_deref() != Some("updates")),
        "session id must not be the filename stem"
    );
}
