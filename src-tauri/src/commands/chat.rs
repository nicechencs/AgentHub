//! Chat Tauri commands — thin wrappers over agenthub-core ChatService.

use agenthub_core::models::{
    AgentId, ChatEvent, ChatHistoryTurn, ChatMessage, Conversation, LiveChatModel,
    MarkdownFilePreview,
};
use agenthub_core::services::chat_cwd::stored_cwd_missing;
use agenthub_core::services::chat_runtime::{
    RuntimeOptions, RuntimeReply, RuntimeSnapshot, RuntimeStartExtras, RuntimeTurnSettings,
};
use agenthub_core::utils::markdown_preview::read_markdown_file_preview;
use agenthub_core::AgentHub;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;

use agenthub_core::logging::targets;

use crate::commands::{map_err_string, parse_agent, with_hub_blocking};
use crate::state::AppState;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationWire {
    #[serde(flatten)]
    conversation: Conversation,
    cwd_missing: bool,
}

fn wire_conversation(conversation: Conversation) -> ConversationWire {
    let cwd_missing = stored_cwd_missing(conversation.cwd.as_deref());
    ConversationWire {
        conversation,
        cwd_missing,
    }
}

/// Invoke: `list_conversations`
#[tauri::command]
pub async fn list_conversations(
    state: State<'_, AppState>,
) -> Result<Vec<ConversationWire>, String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, |hub| {
        list_conversations_inner(hub).map(|rows| rows.into_iter().map(wire_conversation).collect())
    })
    .await
}

/// Invoke: `create_conversation`
#[tauri::command]
pub async fn create_conversation(
    state: State<'_, AppState>,
    agent_ids: Vec<String>,
    cwd: Option<String>,
) -> Result<ConversationWire, String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| {
        create_conversation_inner(hub, agent_ids, cwd_or_home(cwd)).map(wire_conversation)
    })
    .await
}

/// Invoke: `ensure_default_conversation`
#[tauri::command]
pub async fn ensure_default_conversation(
    state: State<'_, AppState>,
    agent_ids: Vec<String>,
    cwd: Option<String>,
) -> Result<ConversationWire, String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| {
        ensure_default_conversation_inner(hub, agent_ids, cwd_or_home(cwd)).map(wire_conversation)
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
) -> Result<ConversationWire, String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| {
        update_conversation_inner(hub, &id, title, agent_ids, cwd, allow_dangerous)
            .map(wire_conversation)
    })
    .await
}

/// Invoke: `refresh_chat_agent_title`
///
/// Adopt the title the Agent wrote in its own session store. Returns the new
/// title when the conversation was retitled, `null` when nothing may change
/// (no Agent title, no session id, or a title the user owns).
#[tauri::command]
pub async fn refresh_chat_agent_title(
    state: State<'_, AppState>,
    conversation_id: String,
) -> Result<Option<String>, String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| {
        refresh_chat_agent_title_inner(hub, &conversation_id)
    })
    .await
}

/// Invoke: `open_conversation_from_session`
#[tauri::command]
pub async fn open_conversation_from_session(
    state: State<'_, AppState>,
    agent_id: String,
    session_id: Option<String>,
    cwd: Option<String>,
    title: Option<String>,
    history: Vec<ChatHistoryTurn>,
) -> Result<ConversationWire, String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| {
        open_conversation_from_session_inner(hub, agent_id, session_id, cwd, title, history)
            .map(wire_conversation)
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

/// Durable polling snapshot for interactive conversations; independent of WebView lifetime.
#[tauri::command]
pub async fn chat_runtime_snapshot(
    state: State<'_, AppState>,
    conversation_id: String,
    after_sequence: Option<i64>,
) -> Result<RuntimeSnapshot, String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| {
        hub.chat()
            .runtime()
            .snapshot(&conversation_id, after_sequence)
            .map_err(|e| map_err_string("chat_runtime_snapshot", e))
    })
    .await
}

#[tauri::command]
pub async fn chat_runtime_options(
    state: State<'_, AppState>,
    conversation_id: String,
    refresh: Option<bool>,
) -> Result<RuntimeOptions, String> {
    let hub = state.hub_arc()?;
    let refresh = refresh.unwrap_or(false);
    with_hub_blocking(hub, move |hub| {
        let runtime = hub.chat().runtime();
        let options = if refresh {
            runtime.refresh_options(&conversation_id)
        } else {
            runtime.options(&conversation_id)
        };
        options.map_err(|e| map_err_string("chat_runtime_options", e))
    })
    .await
}

#[tauri::command]
pub async fn chat_runtime_set_settings(
    state: State<'_, AppState>,
    conversation_id: String,
    settings: RuntimeTurnSettings,
) -> Result<RuntimeTurnSettings, String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| {
        hub.chat()
            .runtime()
            .set_settings(&conversation_id, settings)
            .map_err(|e| map_err_string("chat_runtime_set_settings", e))
    })
    .await
}

#[tauri::command]
pub async fn chat_runtime_note_thinking_failure(
    state: State<'_, AppState>,
    conversation_id: String,
    settings: RuntimeTurnSettings,
    error_text: String,
) -> Result<(), String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| {
        hub.chat()
            .runtime()
            .note_thinking_failure(&conversation_id, settings, &error_text)
            .map_err(|e| map_err_string("chat_runtime_note_thinking_failure", e))
    })
    .await
}

#[tauri::command]
pub async fn chat_runtime_continue_legacy(
    state: State<'_, AppState>,
    conversation_id: String,
) -> Result<RuntimeSnapshot, String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| {
        hub.chat()
            .runtime()
            .continue_legacy(&conversation_id)
            .map_err(|e| map_err_string("chat_runtime_continue_legacy", e))
    })
    .await
}

#[tauri::command]
pub async fn chat_runtime_start(
    state: State<'_, AppState>,
    conversation_id: String,
    prompt: String,
    client_request_id: String,
    extras: Option<RuntimeStartExtras>,
) -> Result<RuntimeSnapshot, String> {
    let hub = state.hub_arc()?;
    let extras = extras.unwrap_or_default();
    with_hub_blocking(hub, move |hub| {
        hub.chat()
            .runtime()
            .start(&conversation_id, &prompt, &client_request_id, extras)
            .map_err(|e| map_err_string("chat_runtime_start", e))
    })
    .await
}

#[tauri::command]
pub async fn chat_runtime_reply(
    state: State<'_, AppState>,
    reply: RuntimeReply,
) -> Result<(), String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| {
        hub.chat()
            .runtime()
            .reply(reply)
            .map_err(|e| map_err_string("chat_runtime_reply", e))
    })
    .await
}

#[tauri::command]
pub async fn chat_runtime_steer(
    state: State<'_, AppState>,
    conversation_id: String,
    run_id: String,
    prompt: String,
    client_request_id: String,
) -> Result<(), String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| {
        hub.chat()
            .runtime()
            .steer(&conversation_id, &run_id, &prompt, &client_request_id)
            .map_err(|e| map_err_string("chat_runtime_steer", e))
    })
    .await
}

#[tauri::command]
pub async fn chat_runtime_cancel(
    state: State<'_, AppState>,
    conversation_id: String,
    run_id: String,
) -> Result<(), String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| {
        hub.chat()
            .runtime()
            .cancel(&conversation_id, &run_id)
            .map_err(|e| map_err_string("chat_runtime_cancel", e))
    })
    .await
}

#[tauri::command]
pub async fn chat_runtime_kill_host_terminal(
    state: State<'_, AppState>,
    conversation_id: String,
    terminal_id: String,
) -> Result<(), String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| {
        hub.chat()
            .runtime()
            .kill_host_terminal(&conversation_id, &terminal_id)
            .map_err(|e| map_err_string("chat_runtime_kill_host_terminal", e))
    })
    .await
}

#[tauri::command]
pub async fn chat_runtime_clear_session_allow_always(
    state: State<'_, AppState>,
    conversation_id: String,
) -> Result<RuntimeSnapshot, String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| {
        hub.chat()
            .runtime()
            .clear_session_allow_always(&conversation_id)
            .map_err(|e| map_err_string("chat_runtime_clear_session_allow_always", e))
    })
    .await
}

/// Invoke: `set_chat_model` — write the live default model for Chat.
#[tauri::command]
pub async fn set_chat_model(
    state: State<'_, AppState>,
    agent_id: String,
    model: String,
) -> Result<(), String> {
    let hub = state.hub_arc()?;
    let agent = parse_agent(&agent_id)?;
    with_hub_blocking(hub, move |hub| {
        hub.set_live_chat_model(agent, &model)
            .map_err(|e| map_err_string("set_chat_model", e))
    })
    .await
}

/// Invoke: `set_chat_effort` — write the live thinking level for Chat.
#[tauri::command]
pub async fn set_chat_effort(
    state: State<'_, AppState>,
    agent_id: String,
    effort: String,
) -> Result<(), String> {
    let hub = state.hub_arc()?;
    let agent = parse_agent(&agent_id)?;
    with_hub_blocking(hub, move |hub| {
        hub.set_live_chat_effort(agent, &effort)
            .map_err(|e| map_err_string("set_chat_effort", e))
    })
    .await
}

/// Invoke: `get_chat_model` — read the live default model and picker ids.
#[tauri::command]
pub async fn get_chat_model(
    state: State<'_, AppState>,
    agent_id: String,
) -> Result<LiveChatModel, String> {
    let hub = state.hub_arc()?;
    let agent = parse_agent(&agent_id)?;
    with_hub_blocking(hub, move |hub| {
        hub.live_chat_model(agent)
            .map_err(|e| map_err_string("get_chat_model", e))
    })
    .await
}

fn list_conversations_inner(hub: &AgentHub) -> Result<Vec<Conversation>, String> {
    hub.chat()
        .list_conversations()
        .map_err(|e| map_err_string("list_conversations", e))
}

/// New chats without a folder start in the user's home so the first message can
/// be sent right away. The UI already passes the active / last folder when it has one.
fn cwd_or_home(cwd: Option<String>) -> Option<String> {
    if cwd.as_deref().is_some_and(|c| !c.trim().is_empty()) {
        return cwd;
    }
    agenthub_core::utils::paths::home_dir()
        .ok()
        .map(|home| home.to_string_lossy().into_owned())
        .or(cwd)
}

fn create_conversation_inner(
    hub: &AgentHub,
    agent_ids: Vec<String>,
    cwd: Option<String>,
) -> Result<Conversation, String> {
    let agents = parse_agent_ids(agent_ids)?;
    hub.chat()
        .create_conversation(agents, cwd)
        .map_err(|e| map_err_string("create_conversation", e))
}

fn ensure_default_conversation_inner(
    hub: &AgentHub,
    agent_ids: Vec<String>,
    cwd: Option<String>,
) -> Result<Conversation, String> {
    let agents = parse_agent_ids(agent_ids)?;
    hub.chat()
        .ensure_default_conversation(agents, cwd)
        .map_err(|e| map_err_string("ensure_default_conversation", e))
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
    hub.chat()
        .update_conversation(id, title, agents, cwd_patch, allow_dangerous)
        .map_err(|e| map_err_string("update_conversation", e))
}

fn refresh_chat_agent_title_inner(hub: &AgentHub, id: &str) -> Result<Option<String>, String> {
    hub.chat()
        .adopt_agent_title(id)
        .map_err(|e| map_err_string("refresh_chat_agent_title", e))
}

fn open_conversation_from_session_inner(
    hub: &AgentHub,
    agent_id: String,
    session_id: Option<String>,
    cwd: Option<String>,
    title: Option<String>,
    history: Vec<ChatHistoryTurn>,
) -> Result<Conversation, String> {
    let agent = parse_agent(&agent_id)?;
    hub.chat()
        .open_from_session(agent, session_id, cwd, title, history)
        .map_err(|e| map_err_string("open_conversation_from_session", e))
}

fn delete_conversation_inner(hub: &AgentHub, id: &str) -> Result<(), String> {
    hub.chat()
        .delete_conversation(id)
        .map_err(|e| map_err_string("delete_conversation", e))
}

fn list_chat_messages_inner(
    hub: &AgentHub,
    conversation_id: &str,
) -> Result<Vec<ChatMessage>, String> {
    hub.chat()
        .list_messages(conversation_id)
        .map_err(|e| map_err_string("list_chat_messages", e))
}

fn chat_cancel_inner(hub: &AgentHub, conversation_id: &str) -> Result<(), String> {
    hub.chat()
        .cancel(conversation_id)
        .map_err(|e| map_err_string("chat_cancel", e))
}

fn chat_send_inner(
    hub: &AgentHub,
    conversation_id: &str,
    prompt: &str,
    on_event: Channel<ChatEvent>,
) -> Result<(), String> {
    hub.chat()
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
mod tests;

/// Invoke: `pick_chat_images` — select one or more local image paths for Codex localImage input.
#[tauri::command]
pub async fn pick_chat_images(
    app: AppHandle,
    title: Option<String>,
) -> Result<Vec<String>, String> {
    let mut dialog = app.dialog().file();
    dialog = dialog.set_title(title.as_deref().unwrap_or("选择图片"));
    dialog = dialog.add_filter("Images", &["png", "jpg", "jpeg", "gif", "webp", "bmp"]);
    if let Some(window) = app.get_webview_window("main") {
        dialog = dialog.set_parent(&window);
    }
    let picked = dialog.blocking_pick_files().unwrap_or_default();
    let mut out = Vec::new();
    for path in picked {
        let buf = path
            .simplified()
            .into_path()
            .map_err(|e| format!("invalid image path: {e}"))?;
        out.push(buf.to_string_lossy().into_owned());
    }
    Ok(out)
}

/// Invoke: `save_chat_paste_image` — persist a clipboard/paste image for Codex localImage.
#[tauri::command]
pub async fn save_chat_paste_image(
    base64: String,
    extension: String,
    byte_length: Option<u64>,
) -> Result<String, String> {
    save_chat_paste_image_inner(&base64, &extension, byte_length)
}

fn save_chat_paste_image_inner(
    base64: &str,
    extension: &str,
    byte_length: Option<u64>,
) -> Result<String, String> {
    use base64::Engine;
    const MAX_BYTES: u64 = 10 * 1024 * 1024;
    let ext = extension
        .trim()
        .trim_start_matches('.')
        .to_ascii_lowercase();
    let allowed = ["png", "jpg", "jpeg", "gif", "webp", "bmp"];
    if !allowed.iter().any(|item| *item == ext) {
        return Err(format!("unsupported image type: {ext}"));
    }
    if let Some(declared) = byte_length {
        if declared > MAX_BYTES {
            return Err("image too large (max 10MB)".into());
        }
    }
    let cleaned: String = base64.chars().filter(|ch| !ch.is_whitespace()).collect();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(cleaned.as_bytes())
        .or_else(|_| base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(cleaned.as_bytes()))
        .map_err(|e| format!("invalid base64 image payload: {e}"))?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("image too large (max 10MB)".into());
    }
    let dir = std::env::temp_dir().join(PASTE_DIR_NAME);
    std::fs::create_dir_all(&dir).map_err(|e| format!("create paste dir: {e}"))?;
    // Best-effort: a failed sweep must never block saving the new image.
    cleanup_stale_paste_images(&dir, std::time::SystemTime::now(), PASTE_IMAGE_MAX_AGE);
    let ext = if ext == "jpeg" {
        "jpg".to_string()
    } else {
        ext
    };
    let path = write_new_paste_image(&dir, &ext, &bytes)?;
    Ok(path.to_string_lossy().into_owned())
}

const PASTE_DIR_NAME: &str = "agenthub-chat-paste";
const PASTE_FILE_PREFIX: &str = "paste-";
const PASTE_SAVED_EXTENSIONS: [&str; 5] = ["png", "jpg", "gif", "webp", "bmp"];

/// Pasted images older than this are removed on the next paste. Screenshots may
/// hold sensitive content, so they must not live forever; but the other Agent
/// can still reference the path in later turns of the same conversation, so the
/// window is deliberately generous (a week) rather than minutes or hours.
const PASTE_IMAGE_MAX_AGE: std::time::Duration = std::time::Duration::from_secs(7 * 24 * 60 * 60);

/// Process-wide sequence so two pastes in the same millisecond never share a name.
static PASTE_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Writes `bytes` to a fresh `paste-<millis>-<pid>-<seq>.<ext>` file in `dir`.
/// Uses `create_new`, so an existing file is never overwritten; on the (unexpected)
/// `AlreadyExists` it retries with the next sequence number.
fn write_new_paste_image(
    dir: &std::path::Path,
    ext: &str,
    bytes: &[u8],
) -> Result<std::path::PathBuf, String> {
    use std::io::Write;
    const MAX_ATTEMPTS: u32 = 16;
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let pid = std::process::id();
    for _ in 0..MAX_ATTEMPTS {
        let seq = PASTE_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = dir.join(format!("{PASTE_FILE_PREFIX}{millis}-{pid}-{seq}.{ext}"));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                if let Err(e) = file.write_all(bytes) {
                    drop(file);
                    let _ = std::fs::remove_file(&path);
                    return Err(format!("write paste image: {e}"));
                }
                return Ok(path);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("write paste image: {e}")),
        }
    }
    Err("write paste image: could not allocate a unique file name".into())
}

/// True when `name` matches what [`write_new_paste_image`] produces
/// (`paste-<digits>-<digits>[-<digits>].<ext>`; the 2-part form is the legacy name).
fn is_generated_paste_name(name: &str) -> bool {
    let Some(rest) = name.strip_prefix(PASTE_FILE_PREFIX) else {
        return false;
    };
    let Some((stem, ext)) = rest.rsplit_once('.') else {
        return false;
    };
    if !PASTE_SAVED_EXTENSIONS.contains(&ext) {
        return false;
    }
    let parts: Vec<&str> = stem.split('-').collect();
    (parts.len() == 2 || parts.len() == 3)
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}

/// Deletes generated paste images in `dir` whose mtime is older than `max_age`
/// relative to `now`. Only regular files directly in `dir` are touched; symlinks
/// (checked via `symlink_metadata`, never followed), subdirectories and files with
/// other names are left alone. All errors are ignored.
fn cleanup_stale_paste_images(
    dir: &std::path::Path,
    now: std::time::SystemTime,
    max_age: std::time::Duration,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if !is_generated_paste_name(name) {
            continue;
        }
        let path = entry.path();
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if !meta.file_type().is_file() {
            continue;
        }
        let Ok(modified) = meta.modified() else {
            continue;
        };
        let expired = now
            .duration_since(modified)
            .map(|age| age > max_age)
            .unwrap_or(false);
        if expired {
            let _ = std::fs::remove_file(&path);
        }
    }
}

/// Invoke: `read_markdown_preview` — load a text/markdown file under the chat working directory.
#[tauri::command]
pub async fn read_markdown_preview(
    path: String,
    cwd: String,
) -> Result<MarkdownFilePreview, String> {
    read_markdown_file_preview(&path, &cwd).map_err(|e| map_err_string("read_markdown_preview", e))
}
