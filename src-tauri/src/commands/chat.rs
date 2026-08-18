//! Chat Tauri commands — thin wrappers over agenthub-core ChatService.

use agenthub_core::models::{AgentId, ChatEvent, ChatMessage, Conversation};
use agenthub_core::AgentHub;
use tauri::ipc::Channel;
use tauri::State;

use agenthub_core::logging::targets;

use crate::commands::{map_err_string, parse_agent, with_hub_blocking};
use crate::state::AppState;

/// Invoke: `list_conversations`
#[tauri::command]
pub async fn list_conversations(state: State<'_, AppState>) -> Result<Vec<Conversation>, String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, list_conversations_inner).await
}

/// Invoke: `create_conversation`
#[tauri::command]
pub async fn create_conversation(
    state: State<'_, AppState>,
    agent_ids: Vec<String>,
    cwd: Option<String>,
) -> Result<Conversation, String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| {
        create_conversation_inner(hub, agent_ids, cwd)
    })
    .await
}

/// Invoke: `update_conversation`
///
/// `cwd`: omit/null = leave unchanged; empty string = clear; non-empty = set.
#[tauri::command]
pub async fn update_conversation(
    state: State<'_, AppState>,
    id: String,
    title: Option<String>,
    agent_ids: Option<Vec<String>>,
    cwd: Option<String>,
    allow_dangerous: Option<bool>,
) -> Result<Conversation, String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| {
        update_conversation_inner(hub, &id, title, agent_ids, cwd, allow_dangerous)
    })
    .await
}

/// Invoke: `delete_conversation`
#[tauri::command]
pub async fn delete_conversation(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| delete_conversation_inner(hub, &id)).await
}

/// Invoke: `list_chat_messages`
#[tauri::command]
pub async fn list_chat_messages(
    state: State<'_, AppState>,
    conversation_id: String,
) -> Result<Vec<ChatMessage>, String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| {
        list_chat_messages_inner(hub, &conversation_id)
    })
    .await
}

/// Invoke: `chat_send` — blocks on CLI subprocesses; runs on blocking pool.
#[tauri::command]
pub async fn chat_send(
    state: State<'_, AppState>,
    conversation_id: String,
    prompt: String,
    on_event: Channel<ChatEvent>,
) -> Result<(), String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| {
        chat_send_inner(hub, &conversation_id, &prompt, on_event)
    })
    .await
}

/// Invoke: `chat_cancel` — lightweight in-memory flag; safe on main thread.
#[tauri::command]
pub fn chat_cancel(state: State<'_, AppState>, conversation_id: String) -> Result<(), String> {
    chat_cancel_inner(state.hub()?, &conversation_id)
}

fn list_conversations_inner(hub: &AgentHub) -> Result<Vec<Conversation>, String> {
    hub.chat
        .list_conversations()
        .map_err(|e| map_err_string("list_conversations", e))
}

fn create_conversation_inner(
    hub: &AgentHub,
    agent_ids: Vec<String>,
    cwd: Option<String>,
) -> Result<Conversation, String> {
    let agents = parse_agent_ids(agent_ids)?;
    hub.chat
        .create_conversation(agents, cwd)
        .map_err(|e| map_err_string("create_conversation", e))
}

fn update_conversation_inner(
    hub: &AgentHub,
    id: &str,
    title: Option<String>,
    agent_ids: Option<Vec<String>>,
    cwd: Option<String>,
    allow_dangerous: Option<bool>,
) -> Result<Conversation, String> {
    let agents = match agent_ids {
        None => None,
        Some(ids) => Some(parse_agent_ids(ids)?),
    };
    let cwd_patch = cwd.map(|c| {
        let t = c.trim();
        if t.is_empty() {
            None
        } else {
            Some(t.to_string())
        }
    });
    hub.chat
        .update_conversation(id, title, agents, cwd_patch, allow_dangerous)
        .map_err(|e| map_err_string("update_conversation", e))
}

fn delete_conversation_inner(hub: &AgentHub, id: &str) -> Result<(), String> {
    hub.chat
        .delete_conversation(id)
        .map_err(|e| map_err_string("delete_conversation", e))
}

fn list_chat_messages_inner(
    hub: &AgentHub,
    conversation_id: &str,
) -> Result<Vec<ChatMessage>, String> {
    hub.chat
        .list_messages(conversation_id)
        .map_err(|e| map_err_string("list_chat_messages", e))
}

fn chat_cancel_inner(hub: &AgentHub, conversation_id: &str) -> Result<(), String> {
    hub.chat
        .cancel(conversation_id)
        .map_err(|e| map_err_string("chat_cancel", e))
}

fn chat_send_inner(
    hub: &AgentHub,
    conversation_id: &str,
    prompt: &str,
    on_event: Channel<ChatEvent>,
) -> Result<(), String> {
    hub.chat
        .send(conversation_id, prompt, &|ev| {
            let _ = on_event.send(ev);
        })
        .map_err(|e| map_err_string("chat_send", e))
}

fn parse_agent_ids(ids: Vec<String>) -> Result<Vec<AgentId>, String> {
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        let agent = parse_agent(&id)?;
        if !out.contains(&agent) {
            out.push(agent);
        }
    }
    if out.is_empty() {
        let msg = "agent list is empty".to_string();
        tracing::warn!(target: targets::GUI, op = "parse_agent_ids", "{msg}");
        return Err(msg);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use agenthub_core::models::AgentId;
    use tempfile::tempdir;

    #[test]
    fn create_list_send_and_delete() {
        let dir = tempdir().unwrap();
        let hub = AgentHub::open(Some(dir.path())).unwrap();
        let conv = create_conversation_inner(&hub, vec!["claude".into()], None).unwrap();
        assert!(!conv.id.is_empty());
        let list = list_conversations_inner(&hub).unwrap();
        assert_eq!(list.len(), 1);

        hub.chat
            .send(&conv.id, "hello from test", &|_ev| {})
            .unwrap();
        let msgs = list_chat_messages_inner(&hub, &conv.id).unwrap();
        assert!(msgs
            .iter()
            .any(|m| matches!(m.role, agenthub_core::models::ChatRole::User)));
        delete_conversation_inner(&hub, &conv.id).unwrap();
        assert!(list_conversations_inner(&hub).unwrap().is_empty());
    }

    #[test]
    fn parse_agent_ids_dedupes_and_rejects_empty() {
        let ids = parse_agent_ids(vec!["claude".into(), "claude".into(), "codex".into()]).unwrap();
        assert_eq!(ids, vec![AgentId::Claude, AgentId::Codex]);

        let err = parse_agent_ids(vec![]).unwrap_err();
        assert!(err.contains("empty"));

        let err = parse_agent_ids(vec!["nope".into()]).unwrap_err();
        assert!(!err.is_empty());
    }

    #[test]
    fn create_dedupes_agents_and_update_roundtrip() {
        let dir = tempdir().unwrap();
        let hub = AgentHub::open(Some(dir.path())).unwrap();
        let conv = create_conversation_inner(
            &hub,
            vec!["claude".into(), "claude".into()],
            None,
        )
        .unwrap();
        assert_eq!(conv.agent_ids, vec![AgentId::Claude]);

        let err = create_conversation_inner(
            &hub,
            vec!["claude".into(), "grok".into()],
            None,
        )
        .unwrap_err();
        assert!(err.contains("only one agent"));

        let updated = update_conversation_inner(
            &hub,
            &conv.id,
            Some("title".into()),
            Some(vec!["codex".into()]),
            Some(String::new()), // clear cwd
            Some(true),
        )
        .unwrap();
        assert_eq!(updated.title, "title");
        assert_eq!(updated.agent_ids, vec![AgentId::Codex]);
        assert!(updated.cwd.is_none());
        assert!(updated.allow_dangerous);
    }

    #[test]
    fn cancel_is_noop_without_inflight_send() {
        let dir = tempdir().unwrap();
        let hub = AgentHub::open(Some(dir.path())).unwrap();
        let conv = create_conversation_inner(&hub, vec!["claude".into()], None).unwrap();
        chat_cancel_inner(&hub, &conv.id).unwrap();
    }

    #[test]
    fn empty_agent_list_rejected_on_create() {
        let dir = tempdir().unwrap();
        let hub = AgentHub::open(Some(dir.path())).unwrap();
        let err = create_conversation_inner(&hub, vec![], None).unwrap_err();
        assert!(err.contains("empty") || err.contains("at least"));
    }
}
