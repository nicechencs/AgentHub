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
