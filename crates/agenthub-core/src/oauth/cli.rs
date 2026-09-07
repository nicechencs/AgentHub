//! Official login by spawning the agent's CLI (`kiro-cli login`), then importing.
//!
//! Device-flow stdout is parsed so the wait page can show the same login link
//! (copy / open browser) as Claude / Grok. The hidden CLI process is not relied
//! on to open a browser by itself.

use std::collections::HashMap;
use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::error::{AppError, Result};
use crate::logging::targets;
use crate::models::{AccountInput, AccountKind, AgentId, LiveAccount};
use crate::services::AccountService;
use crate::utils::process::{
    apply_no_window, configure_process_group, poll_child, CancelToken, ChildPoll, ProcessControl,
};

use super::pkce;
use super::session::OAuthSession;
use super::{store, StartOAuthResult};

const KIRO_LOGIN_SENTINEL: &str = "kiro-cli-login";
const LOGIN_PROMPT_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, PartialEq, Eq)]
struct KiroLoginPrompt {
    url: String,
    user_code: Option<String>,
}

struct KiroCliJob {
    cancel: CancelToken,
    previous: Option<LiveAccount>,
}

fn cancel_registry() -> &'static Mutex<HashMap<String, KiroCliJob>> {
    static REGISTRY: std::sync::OnceLock<Mutex<HashMap<String, KiroCliJob>>> =
        std::sync::OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn cancel_kiro_cli_login(state: &str) {
    abort_kiro_cli_job(state, true);
}

fn release_kiro_cli_login(state: &str) {
    abort_kiro_cli_job(state, false);
}

fn abort_kiro_cli_job(state: &str, restore_previous: bool) {
    let Ok(mut jobs) = cancel_registry().lock() else {
        return;
    };
    let Some(job) = jobs.remove(state) else {
        return;
    };
    job.cancel.cancel();
    if restore_previous {
        if let Some(previous) = job.previous {
            let _ = crate::adapters::kiro::write_kiro_live_account(&previous);
        }
    }
}

pub fn start_kiro_cli_login(accounts: Option<&AccountService>) -> Result<StartOAuthResult> {
    let bin = kiro_cli_binary_for_login()
        .ok_or_else(|| AppError::Unsupported("请先安装 Kiro 命令行".into()))?;

    let before = crate::adapters::kiro::kiro_login_fingerprint();
    let state = pkce::random_state()?;
    let st = store();
    st.insert(OAuthSession::new(
        state.clone(),
        AgentId::Kiro,
        String::new(),
        String::new(),
        Some("kiro".into()),
    ))?;

    let previous = match preserve_then_logout_if_needed(accounts, before.is_some()) {
        Ok(previous) => previous,
        Err(error) => {
            let _ = st.mark_error(&state, "logout failed");
            return Err(error);
        }
    };

    let cancel = CancelToken::new();
    {
        let mut jobs = cancel_registry()
            .lock()
            .map_err(|_| AppError::message("oauth.store", "kiro login store poisoned"))?;
        jobs.insert(
            state.clone(),
            KiroCliJob {
                cancel: cancel.clone(),
                previous,
            },
        );
    }

    let mut cmd = Command::new(&bin);
    cmd.args(["login", "--license", "free", "--use-device-flow"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("TERM", "dumb")
        .env("NO_COLOR", "1")
        .env("CLICOLOR", "0");
    apply_no_window(&mut cmd);
    let process_control = match configure_process_group(&mut cmd) {
        Ok(control) => control,
        Err(_) => {
            fail_start(&state);
            return Err(AppError::message("oauth.kiro", "无法打开 Kiro 登录"));
        }
    };
    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(_) => {
            fail_start(&state);
            return Err(AppError::message("oauth.kiro", "无法打开 Kiro 登录"));
        }
    };
    if process_control.attach(&child).is_err() {
        let _ = child.kill();
        process_control.terminate(&mut child);
        fail_start(&state);
        return Err(AppError::message("oauth.kiro", "无法打开 Kiro 登录"));
    }

    let output = Arc::new(Mutex::new(String::new()));
    spawn_output_reader(&mut child, Arc::clone(&output), cancel.clone());

    let prompt = wait_for_login_prompt(&state, &st, &cancel, &mut child, &process_control, &output);

    match prompt {
        WaitPrompt::Url(prompt) => {
            let state_for_thread = state.clone();
            let cancel = cancel.clone();
            thread::spawn(move || {
                watch_kiro_login(state_for_thread, None, cancel, child, process_control);
            });
            Ok(StartOAuthResult {
                state,
                authorize_url: prompt.url,
                redirect_uri: String::new(),
                agent_id: AgentId::Kiro,
                provider_key: Some("kiro".into()),
                browser_opened: false,
                expires_in_secs: crate::catalog::limits::OAUTH_CALLBACK_LISTEN_TIMEOUT.as_secs(),
                user_code: prompt.user_code,
            })
        }
        WaitPrompt::AlreadyReady => {
            process_control.terminate(&mut child);
            let _ = child.wait();
            Ok(ready_start(state))
        }
        WaitPrompt::Failed => {
            let output_text = output.lock().ok().map(|s| s.clone()).unwrap_or_default();
            tracing::warn!(
                module = targets::OAUTH,
                op = "kiro_cli_login",
                output = %truncate_output(&output_text),
                "kiro-cli login produced no login link"
            );
            process_control.terminate(&mut child);
            let _ = child.wait();
            fail_start(&state);
            Err(AppError::message("oauth.kiro", "无法打开 Kiro 登录"))
        }
    }
}

fn fail_start(state: &str) {
    let _ = store().mark_error(state, "spawn failed");
    cancel_kiro_cli_login(state);
}

fn preserve_then_logout_if_needed(
    accounts: Option<&AccountService>,
    logged_in: bool,
) -> Result<Option<LiveAccount>> {
    if !logged_in {
        return Ok(None);
    }
    let Some(accounts) = accounts else {
        return Err(AppError::Unsupported(
            "本机已有 Kiro 登录。请到连接页再登录一份，以便先收下当前这份。".into(),
        ));
    };
    let previous = crate::adapters::kiro::read_kiro_live_account().ok();
    accounts
        .import_live(AgentId::Kiro, None)
        .map_err(|_| AppError::message("oauth.kiro", "无法收下当前 Kiro 登录，未开始新的登录"))?;
    kiro_cli_logout()?;
    tracing::info!(
        module = targets::OAUTH,
        op = "kiro_cli_login",
        "preserved current Kiro login and signed out for a new login"
    );
    Ok(previous)
}

fn kiro_cli_logout() -> Result<()> {
    let bin = crate::adapters::kiro::kiro_cli_binary()
        .ok_or_else(|| AppError::Unsupported("请先安装 Kiro 命令行".into()))?;
    let out =
        crate::utils::process::run_capture_timeout(&bin, &["logout"], Duration::from_secs(45))
            .map_err(|_| AppError::message("oauth.kiro", "无法退出当前 Kiro 登录"))?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    if out.status.success() || already_logged_out(&text) {
        Ok(())
    } else {
        Err(AppError::message("oauth.kiro", "无法退出当前 Kiro 登录"))
    }
}

fn already_logged_out(text: &str) -> bool {
    let lower = strip_ansi(text).to_ascii_lowercase();
    lower.contains("not logged in") || lower.contains("not authenticated")
}

fn ready_start(state: String) -> StartOAuthResult {
    StartOAuthResult {
        state,
        authorize_url: String::new(),
        redirect_uri: String::new(),
        agent_id: AgentId::Kiro,
        provider_key: Some("kiro".into()),
        browser_opened: false,
        expires_in_secs: crate::catalog::limits::OAUTH_CALLBACK_LISTEN_TIMEOUT.as_secs(),
        user_code: None,
    }
}

fn truncate_output(text: &str) -> String {
    let flat: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let trimmed = flat.trim();
    if trimmed.chars().count() <= 240 {
        trimmed.to_string()
    } else {
        format!("{}…", trimmed.chars().take(240).collect::<String>())
    }
}

fn kiro_cli_binary_for_login() -> Option<std::path::PathBuf> {
    #[cfg(test)]
    {
        if std::env::var_os("AGENTHUB_TEST_ALLOW_KIRO_LOGIN").is_none() {
            return None;
        }
    }
    crate::adapters::kiro::kiro_cli_binary()
}

enum WaitPrompt {
    Url(KiroLoginPrompt),
    AlreadyReady,
    Failed,
}

fn wait_for_login_prompt(
    state: &str,
    st: &std::sync::Arc<super::session::SessionStore>,
    cancel: &CancelToken,
    child: &mut Child,
    process_control: &ProcessControl,
    output: &Arc<Mutex<String>>,
) -> WaitPrompt {
    let deadline = Instant::now() + LOGIN_PROMPT_TIMEOUT;
    loop {
        if cancel.is_cancelled() || !st.is_waiting(state) {
            return WaitPrompt::Failed;
        }
        if let Some(prompt) = snapshot_prompt(output) {
            return WaitPrompt::Url(prompt);
        }
        match poll_child(child, process_control) {
            Ok(ChildPoll::Exited(_status)) => {
                if let Some(prompt) = snapshot_prompt(output) {
                    return WaitPrompt::Url(prompt);
                }
                let output_text = output.lock().ok().map(|s| s.clone()).unwrap_or_default();
                let fingerprint = crate::adapters::kiro::kiro_login_fingerprint();
                if fingerprint.is_some() || already_logged_in(&output_text) {
                    let _ = st.set_code(state, KIRO_LOGIN_SENTINEL.to_string());
                    cancel_kiro_cli_login(state);
                    return WaitPrompt::AlreadyReady;
                }
                return WaitPrompt::Failed;
            }
            Ok(ChildPoll::Running) => {
                if Instant::now() >= deadline {
                    if let Some(prompt) = snapshot_prompt(output) {
                        return WaitPrompt::Url(prompt);
                    }
                    return WaitPrompt::Failed;
                }
                thread::sleep(Duration::from_millis(100));
            }
            Err(_) => return WaitPrompt::Failed,
        }
    }
}

fn snapshot_prompt(output: &Arc<Mutex<String>>) -> Option<KiroLoginPrompt> {
    let text = output.lock().ok()?;
    parse_kiro_login_prompt(&text)
}

fn spawn_output_reader(child: &mut Child, output: Arc<Mutex<String>>, cancel: CancelToken) {
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    if let Some(out) = stdout {
        let buf = Arc::clone(&output);
        let cancel = cancel.clone();
        thread::spawn(move || drain_pipe(out, buf, cancel));
    }
    if let Some(err) = stderr {
        thread::spawn(move || drain_pipe(err, output, cancel));
    }
}

fn drain_pipe<R: Read>(mut pipe: R, output: Arc<Mutex<String>>, cancel: CancelToken) {
    let mut chunk = [0_u8; 4096];
    while !cancel.is_cancelled() {
        match pipe.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                if let Ok(mut buf) = output.lock() {
                    buf.push_str(&String::from_utf8_lossy(&chunk[..n]));
                    if buf.len() > 64 * 1024 {
                        let drain = buf.len() - 32 * 1024;
                        buf.drain(..drain);
                    }
                }
            }
            Err(_) => break,
        }
    }
    let _ = pipe;
}

fn watch_kiro_login(
    state: String,
    before: Option<String>,
    cancel: CancelToken,
    mut child: Child,
    process_control: ProcessControl,
) {
    let st = store();
    let outcome = loop {
        if cancel.is_cancelled() || !st.is_waiting(&state) {
            process_control.terminate(&mut child);
            let _ = child.wait();
            break None;
        }
        let fingerprint = crate::adapters::kiro::kiro_login_fingerprint();
        if fingerprint.is_some() && fingerprint != before {
            process_control.terminate(&mut child);
            let _ = child.wait();
            break Some(true);
        }
        match poll_child(&mut child, &process_control) {
            Ok(ChildPoll::Running) => {
                thread::sleep(Duration::from_millis(400));
            }
            Ok(ChildPoll::Exited(status)) => {
                let ok = status.map(|s| s.success()).unwrap_or(true);
                let fingerprint = crate::adapters::kiro::kiro_login_fingerprint();
                break Some(ok && fingerprint.is_some());
            }
            Err(_) => break Some(false),
        }
    };
    if !st.is_waiting(&state) {
        release_kiro_cli_login(&state);
        return;
    }
    match outcome {
        Some(true) => {
            release_kiro_cli_login(&state);
            let _ = st.set_code(&state, KIRO_LOGIN_SENTINEL.to_string());
        }
        Some(false) => {
            cancel_kiro_cli_login(&state);
            let _ = st.mark_error(&state, "login failed");
        }
        None => {
            release_kiro_cli_login(&state);
        }
    }
}

pub fn complete_kiro_cli_login(
    accounts: &crate::services::AccountService,
    state: &str,
) -> Result<crate::models::Account> {
    let st = store();
    let session = st.take_ready(state)?;
    if session.agent() != AgentId::Kiro {
        let _ = st.mark_completion_failed(state);
        return Err(AppError::InvalidArg("not a Kiro login session".into()));
    }
    release_kiro_cli_login(state);
    let live = match crate::adapters::kiro::read_kiro_live_account() {
        Ok(live) => live,
        Err(error) => {
            let _ = st.mark_completion_failed(state);
            return Err(error);
        }
    };
    let label = live.label_hint.clone().unwrap_or_else(|| "Kiro".into());
    match accounts.create(AccountInput {
        agent_id: AgentId::Kiro,
        kind: AccountKind::Oauth,
        label,
        credentials: live.credentials,
        extra: live.extra,
        is_current: false,
    }) {
        Ok(account) => {
            let _ = st.mark_succeeded(state);
            Ok(account)
        }
        Err(error) => {
            let _ = st.mark_completion_failed(state);
            Err(error)
        }
    }
}

fn already_logged_in(text: &str) -> bool {
    let lower = strip_ansi(text).to_ascii_lowercase();
    lower.contains("already logged in") || lower.contains("please logout")
}

fn parse_kiro_login_prompt(raw: &str) -> Option<KiroLoginPrompt> {
    let text = strip_ansi(raw);
    let url = extract_login_url(&text)?;
    Some(KiroLoginPrompt {
        url,
        user_code: extract_user_code(&text),
    })
}

fn strip_ansi(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for next in chars.by_ref() {
                    if next.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        out.push(ch);
    }
    out
}

fn extract_login_url(text: &str) -> Option<String> {
    for raw in text.split_whitespace() {
        let trimmed = raw.trim_matches(|c: char| {
            matches!(c, '.' | ',' | ')' | ']' | '"' | '\'' | '>' | '<' | ';')
        });
        if !(trimmed.starts_with("https://") || trimmed.starts_with("http://")) {
            continue;
        }
        if is_loopback_url(trimmed) {
            continue;
        }
        return Some(trimmed.to_string());
    }
    None
}

fn is_loopback_url(url: &str) -> bool {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);
    let host = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    let host = host.split('@').next_back().unwrap_or(host);
    let host = host.rsplit_once(':').map(|(h, _)| h).unwrap_or(host);
    matches!(
        host.to_ascii_lowercase().as_str(),
        "127.0.0.1" | "localhost" | "::1" | "[::1]"
    )
}

fn extract_user_code(text: &str) -> Option<String> {
    let mut best = None;
    for token in text.split_whitespace() {
        let trimmed = token.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '-');
        if is_device_user_code(trimmed) {
            best = Some(trimmed.to_ascii_uppercase());
        }
    }
    best
}

fn is_device_user_code(token: &str) -> bool {
    let parts: Vec<&str> = token.split('-').collect();
    if parts.len() != 2 {
        return false;
    }
    parts.iter().all(|part| {
        (4..=8).contains(&part.len()) && part.chars().all(|c| c.is_ascii_alphanumeric())
    })
}

#[cfg(test)]
mod tests;
