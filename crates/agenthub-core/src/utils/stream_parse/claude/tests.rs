use super::*;
use crate::models::{AgentId, OutputStream, ProcessMode, ProcessStep};
use crate::utils::stream_parse::{StreamOutput, StreamSession};

#[test]
fn parses_system_init() {
    let steps = parse_line(r#"{"type":"system","subtype":"init","session_id":"x"}"#).unwrap();
    assert!(matches!(
        &steps[0],
        ProcessStep::Status { phase, .. } if phase == "starting"
    ));
}

#[test]
fn parses_text_delta() {
    let steps =
        parse_line(r#"{"type":"content_block_delta","delta":{"type":"text_delta","text":"ab"}}"#)
            .unwrap();
    assert_eq!(steps, vec![ProcessStep::Text { text: "ab".into() }]);
}

#[test]
fn result_success_includes_final_text_fallback() {
    let steps =
        parse_line(r#"{"type":"result","subtype":"success","result":"final answer"}"#).unwrap();
    assert!(steps.iter().any(|s| matches!(
        s,
        ProcessStep::Text { text } if text == "final answer"
    )));
}

#[test]
fn assistant_then_result_does_not_double_text_in_session() {
    let mut s = StreamSession::new(AgentId::Claude, ProcessMode::Auto);
    let ndjson = concat!(
        r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"PONGC"}]}}"#,
        "\n",
        r#"{"type":"result","subtype":"success","result":"PONGC"}"#,
        "\n",
    );
    let out = s.feed(OutputStream::Stdout, ndjson);
    assert_eq!(s.assistant_text(), "PONGC");
    assert!(out.iter().any(|o| matches!(
        o,
        StreamOutput::Step(ProcessStep::Status { phase, .. }) if phase == "result"
    )));
    let text_chunks: Vec<_> = out
        .iter()
        .filter_map(|o| match o {
            StreamOutput::Chunk { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(text_chunks, vec!["PONGC"]);
}

#[test]
fn result_only_still_fills_assistant_text() {
    let mut s = StreamSession::new(AgentId::Claude, ProcessMode::Auto);
    let line = r#"{"type":"result","subtype":"success","result":"only-result"}"#;
    let _ = s.feed(OutputStream::Stdout, &format!("{line}\n"));
    assert_eq!(s.assistant_text(), "only-result");
}

#[test]
fn deltas_then_result_does_not_double_assistant_text() {
    let mut s = StreamSession::new(AgentId::Claude, ProcessMode::Auto);
    let ndjson = concat!(
        r#"{"type":"content_block_delta","delta":{"type":"text_delta","text":"PONG"}}"#,
        "\n",
        r#"{"type":"content_block_delta","delta":{"type":"text_delta","text":"C"}}"#,
        "\n",
        r#"{"type":"result","subtype":"success","result":"PONGC"}"#,
        "\n",
    );
    let _ = s.feed(OutputStream::Stdout, ndjson);
    assert_eq!(s.assistant_text(), "PONGC");
}

#[test]
fn todo_write_is_plan_not_process_step() {
    let payload = serde_json::json!({
        "type": "assistant",
        "message": {
            "role": "assistant",
            "content": [{
                "type": "tool_use",
                "id": "toolu_1",
                "name": "TodoWrite",
                "input": {
                    "todos": [
                        { "content": "read", "status": "completed", "priority": "high" },
                        { "content": "edit", "status": "in_progress" },
                        { "content": "  " },
                        { "content": "test", "status": "pending" }
                    ]
                }
            }]
        }
    });
    let steps = parse_line(&payload.to_string()).unwrap();
    assert!(steps.is_empty(), "{steps:?}");
    let ops = super::extract_todo_plan(&payload);
    assert_eq!(ops.len(), 1);
    match &ops[0] {
        super::ClaudePlanOp::Replace(entries) => {
            assert_eq!(entries.len(), 3);
            assert_eq!(entries[0].content, "read");
            assert_eq!(entries[0].status.as_deref(), Some("completed"));
            assert_eq!(entries[1].status.as_deref(), Some("in_progress"));
            assert_eq!(entries[2].content, "test");
        }
        other => panic!("expected replace, got {other:?}"),
    }
}

#[test]
fn task_create_and_update_are_plan_patches() {
    let create = serde_json::json!({
        "type": "assistant",
        "message": {
            "content": [{
                "type": "tool_use",
                "id": "toolu_create",
                "name": "TaskCreate",
                "input": { "subject": "build auth", "activeForm": "Building auth" }
            }]
        }
    });
    let ops = super::extract_todo_plan(&create);
    assert_eq!(
        ops,
        vec![super::ClaudePlanOp::Create {
            tool_use_id: Some("toolu_create".into()),
            content: "build auth".into(),
            status: Some("pending".into()),
            id: None,
            priority: None,
        }]
    );
    let update = serde_json::json!({
        "type": "tool_use",
        "id": "toolu_upd",
        "name": "TaskUpdate",
        "input": { "taskId": "task-1", "status": "in_progress" }
    });
    assert_eq!(
        super::extract_todo_plan(&update),
        vec![super::ClaudePlanOp::Update {
            id: "task-1".into(),
            status: Some("in_progress".into()),
            content: None,
            priority: None,
        }]
    );
}

#[test]
fn task_create_result_binds_id() {
    let payload = serde_json::json!({
        "type": "user",
        "tool_use_result": { "task": { "id": "task-9", "subject": "build auth" } },
        "message": {
            "content": [{
                "type": "tool_result",
                "tool_use_id": "toolu_create",
                "content": "created"
            }]
        }
    });
    assert!(parse_line(&payload.to_string()).unwrap().is_empty());
    assert_eq!(
        super::extract_todo_plan(&payload),
        vec![super::ClaudePlanOp::BindId {
            tool_use_id: "toolu_create".into(),
            id: "task-9".into(),
        }]
    );
}

#[test]
fn todo_write_result_is_not_process_step() {
    let payload = serde_json::json!({
        "type": "user",
        "tool_use_result": {
            "oldTodos": [{ "content": "read", "status": "pending" }],
            "newTodos": [{ "content": "read", "status": "completed" }]
        },
        "message": {
            "content": [{
                "type": "tool_result",
                "tool_use_id": "toolu_todo",
                "content": "Todos have been modified successfully."
            }]
        }
    });
    assert!(parse_line(&payload.to_string()).unwrap().is_empty());
}

#[test]
fn task_list_result_replaces_plan() {
    let payload = serde_json::json!({
        "type": "user",
        "tool_use_result": {
            "tasks": [
                { "id": "t1", "subject": "read", "status": "completed" },
                { "id": "t2", "subject": "edit", "status": "in_progress" }
            ]
        },
        "message": {
            "content": [{
                "type": "tool_result",
                "tool_use_id": "toolu_list",
                "content": "listed"
            }]
        }
    });
    assert!(parse_line(&payload.to_string()).unwrap().is_empty());
    match &super::extract_todo_plan(&payload)[0] {
        super::ClaudePlanOp::Replace(entries) => {
            assert_eq!(entries.len(), 2);
            assert_eq!(entries[0].id.as_deref(), Some("t1"));
            assert_eq!(entries[1].content, "edit");
        }
        other => panic!("expected replace, got {other:?}"),
    }
}

#[test]
fn empty_todo_write_is_replace_with_no_rows() {
    let payload = serde_json::json!({
        "type": "assistant",
        "message": {
            "content": [{
                "type": "tool_use",
                "name": "TodoWrite",
                "input": { "todos": [] }
            }]
        }
    });
    assert_eq!(
        super::extract_todo_plan(&payload),
        vec![super::ClaudePlanOp::Replace(vec![])]
    );
}

#[test]
fn bash_tool_use_still_emits_process_step() {
    let steps = parse_line(
        r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"ls"}}]}}"#,
    )
    .unwrap();
    assert!(matches!(
        &steps[0],
        ProcessStep::Tool { name, .. } if name == "Bash"
    ));
    assert!(super::extract_todo_plan(&serde_json::json!({
        "type": "assistant",
        "message": { "content": [{ "type": "tool_use", "name": "Bash", "input": { "command": "ls" } }] }
    }))
    .is_empty());
}

#[test]
fn hyphenated_plan_tool_names_and_nested_event_unwrap() {
    assert!(super::is_claude_plan_tool_name("Todo-Write"));
    assert!(super::is_claude_plan_tool_name("Task Create"));
    assert!(super::is_claude_plan_tool_name("todo_read"));
    assert!(!super::is_claude_plan_tool_name("Bash"));
    let payload = serde_json::json!({
        "event": {
            "type": "assistant",
            "message": {
                "content": [{
                    "type": "tool_use",
                    "id": "toolu_todo",
                    "name": "Todo-Write",
                    "input": {
                        "todos": [
                            { "title": "read docs", "status": "pending" },
                            { "content": "  " }
                        ]
                    }
                }]
            }
        }
    });
    match &super::extract_todo_plan(&payload)[0] {
        super::ClaudePlanOp::Replace(entries) => {
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].content, "read docs");
            assert_eq!(entries[0].status.as_deref(), Some("pending"));
        }
        other => panic!("expected replace, got {other:?}"),
    }
    assert_eq!(
        super::plan_tool_use_ids(payload.get("event").expect("event")),
        vec!["toolu_todo".to_string()]
    );
}

#[test]
fn content_block_task_create_and_skipped_read_tools() {
    let create = serde_json::json!({
        "type": "content_block_start",
        "content_block": {
            "type": "tool_use",
            "id": "toolu_create",
            "name": "TaskCreate",
            "input": { "title": "build auth", "priority": "high" }
        }
    });
    assert_eq!(
        super::extract_todo_plan(&create),
        vec![super::ClaudePlanOp::Create {
            tool_use_id: Some("toolu_create".into()),
            content: "build auth".into(),
            status: Some("pending".into()),
            id: None,
            priority: Some("high".into()),
        }]
    );
    assert!(super::extract_todo_plan(&serde_json::json!({
        "type": "tool_use",
        "name": "TodoRead",
        "input": { "todos": [{ "content": "ignore" }] }
    }))
    .is_empty());
    assert!(super::extract_todo_plan(&serde_json::json!({
        "type": "tool_use",
        "name": "TaskUpdate",
        "input": { "status": "in_progress" }
    }))
    .is_empty());
    assert!(super::extract_todo_plan(&serde_json::json!({
        "type": "tool_use",
        "name": "TaskCreate",
        "input": { "status": "pending" }
    }))
    .is_empty());
}

#[test]
fn task_update_accepts_task_id_alias() {
    let update = serde_json::json!({
        "type": "tool_use",
        "name": "TaskUpdate",
        "input": { "task_id": "task-3", "subject": "rewrite", "status": "completed" }
    });
    assert_eq!(
        super::extract_todo_plan(&update),
        vec![super::ClaudePlanOp::Update {
            id: "task-3".into(),
            status: Some("completed".into()),
            content: Some("rewrite".into()),
            priority: None,
        }]
    );
}

// --- `--include-partial-messages` (fixtures captured from Claude Code 2.1.283, trimmed) ---

const PARTIAL_TEXT: &str = include_str!("fixtures/partial_text.ndjson");
const PARTIAL_THINKING_TOOL: &str = include_str!("fixtures/partial_thinking_tool.ndjson");
const PARTIAL_INTERRUPT: &str = include_str!("fixtures/partial_interrupt.ndjson");

fn fixture_values(ndjson: &str) -> Vec<serde_json::Value> {
    ndjson
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("fixture line is JSON"))
        .collect()
}

/// Mirrors the intended ChatRuntime wiring: dedup, then stateless `parse_line`.
/// `result` text is excluded (ChatRuntime only uses it as a fallback for an empty bubble).
fn dedup_steps(values: &[serde_json::Value]) -> Vec<ProcessStep> {
    let mut dedup = ClaudePartialDedup::new();
    let mut steps = Vec::new();
    for v in values {
        let Some(pass) = dedup.filter(v) else {
            continue;
        };
        if pass.get("type").and_then(|t| t.as_str()) == Some("result") {
            continue;
        }
        if let Some(s) = parse_line(&pass.to_string()) {
            steps.extend(s);
        }
    }
    steps
}

fn joined_text(steps: &[ProcessStep]) -> String {
    steps
        .iter()
        .filter_map(|s| match s {
            ProcessStep::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

#[test]
fn partial_fixture_shapes_match_verified_protocol() {
    let values = fixture_values(PARTIAL_TEXT);
    let start_id = values
        .iter()
        .find_map(partial_message_start_id)
        .expect("message_start id");
    let deltas: String = values.iter().filter_map(partial_text_delta).collect();
    assert_eq!(deltas, "Hi! What can I help you with?");
    let assistants: Vec<_> = values.iter().filter_map(assistant_message_id).collect();
    assert_eq!(assistants, vec![start_id.clone()]);
    // Full assistant line is still emitted with the same text => needs dedup.
    let full = values
        .iter()
        .find(|v| assistant_message_id(v).is_some())
        .unwrap();
    assert_eq!(
        full.pointer("/message/content/0/text").unwrap(),
        "Hi! What can I help you with?"
    );
    assert!(values.iter().all(|v| !is_subagent_line(v)));
}

#[test]
fn partial_delta_helpers_ignore_other_lines() {
    let text = serde_json::json!({"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"ab"}}});
    assert_eq!(partial_text_delta(&text).as_deref(), Some("ab"));
    assert_eq!(partial_thinking_delta(&text), None);
    let thinking = serde_json::json!({"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"hmm","estimated_tokens":null}}});
    assert_eq!(partial_thinking_delta(&thinking).as_deref(), Some("hmm"));
    let empty = serde_json::json!({"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":""}}});
    assert_eq!(partial_thinking_delta(&empty), None);
    let json_delta = serde_json::json!({"type":"stream_event","event":{"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"a\""}}});
    assert_eq!(partial_text_delta(&json_delta), None);
    // Legacy bare content_block_delta is not a partial stream_event.
    let bare =
        serde_json::json!({"type":"content_block_delta","delta":{"type":"text_delta","text":"ab"}});
    assert_eq!(partial_text_delta(&bare), None);
}

#[test]
fn stream_event_text_delta_still_parses_via_parse_line() {
    let steps = parse_line(
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hi"}}}"#,
    )
    .unwrap();
    assert_eq!(steps, vec![ProcessStep::Text { text: "Hi".into() }]);
}

#[test]
fn without_dedup_partial_text_would_double() {
    let values = fixture_values(PARTIAL_TEXT);
    let mut steps = Vec::new();
    for v in values.iter().filter(|v| v["type"] != "result") {
        if let Some(s) = parse_line(&v.to_string()) {
            steps.extend(s);
        }
    }
    assert_eq!(
        joined_text(&steps),
        "Hi! What can I help you with?Hi! What can I help you with?"
    );
}

#[test]
fn dedup_emits_partial_text_once() {
    let steps = dedup_steps(&fixture_values(PARTIAL_TEXT));
    assert_eq!(joined_text(&steps), "Hi! What can I help you with?");
}

#[test]
fn dedup_keeps_tool_use_and_drops_streamed_blocks() {
    let steps = dedup_steps(&fixture_values(PARTIAL_THINKING_TOOL));
    assert_eq!(
        joined_text(&steps),
        "17 × 23 is 391, and `echo probe` printed `probe`."
    );
    let tool_starts = steps
        .iter()
        .filter(|s| match s {
            ProcessStep::Tool {
                name,
                status,
                input,
                ..
            } => name == "Bash" && status == "start" && input.is_some(),
            _ => false,
        })
        .count();
    // Tool input comes from the complete assistant tool_use block (input_json_delta is ignored).
    assert_eq!(tool_starts, 1);
    // Real thinking deltas were empty (redacted display) => no thinking steps.
    assert!(!steps
        .iter()
        .any(|s| matches!(s, ProcessStep::Thinking { .. })));
}

#[test]
fn dedup_strips_streamed_thinking_and_keeps_unseen_tail() {
    let lines = [
        r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_1","content":[]}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"plan"}}}"#,
        r#"{"type":"assistant","message":{"id":"msg_1","content":[{"type":"thinking","thinking":"plan","signature":"s"}]}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_start","index":1,"content_block":{"type":"text","text":""}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"Hel"}}}"#,
        r#"{"type":"assistant","message":{"id":"msg_1","content":[{"type":"text","text":"Hello"}]}}"#,
    ];
    let values: Vec<serde_json::Value> = lines
        .iter()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let steps = dedup_steps(&values);
    assert_eq!(joined_text(&steps), "Hello");
    let thinking: Vec<_> = steps
        .iter()
        .filter_map(|s| match s {
            ProcessStep::Thinking { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(thinking, vec!["plan"]);
}

#[test]
fn dedup_passes_through_without_partial_events() {
    let line = serde_json::json!({"type":"assistant","message":{"id":"msg_x","content":[{"type":"text","text":"PONGC"}]}});
    let mut dedup = ClaudePartialDedup::new();
    assert_eq!(dedup.filter(&line), Some(line.clone()));
    // Different message id than the streamed one is also untouched.
    let start = serde_json::json!({"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_other"}}});
    assert_eq!(dedup.filter(&start), None);
    assert_eq!(dedup.filter(&line), Some(line));
}

#[test]
fn interrupt_fixture_control_protocol() {
    let values = fixture_values(PARTIAL_INTERRUPT);
    let ack = values
        .iter()
        .find_map(control_response_success_id)
        .expect("control_response success");
    assert_eq!(ack, "int-1");
    let results: Vec<_> = values.iter().filter(|v| v["type"] == "result").collect();
    assert_eq!(results.len(), 2);
    assert!(is_interrupted_result(results[0]));
    assert_eq!(results[0]["subtype"], "error_during_execution");
    assert!(!is_interrupted_result(results[1]));
    assert_eq!(results[1]["subtype"], "success");

    let sent: serde_json::Value = serde_json::from_str(&interrupt_request_line("int-1")).unwrap();
    assert_eq!(
        sent,
        serde_json::json!({"type":"control_request","request_id":"int-1","request":{"subtype":"interrupt"}})
    );
}

#[test]
fn dedup_drops_interrupt_snapshot_and_keeps_next_turn() {
    let values = fixture_values(PARTIAL_INTERRUPT);
    let first_result = values.iter().position(|v| v["type"] == "result").unwrap();
    let streamed: String = values[..first_result]
        .iter()
        .filter_map(partial_text_delta)
        .collect();
    assert!(streamed.starts_with("**The Last Light at Skerry Point**"));

    let first_turn = dedup_steps(&values[..=first_result]);
    assert_eq!(joined_text(&first_turn), streamed);
    let second_turn = dedup_steps(&values[first_result + 1..]);
    assert_eq!(
        joined_text(&second_turn),
        "Hi! Want me to finish the lighthouse keeper story, or is there something else you'd like?"
    );
}
