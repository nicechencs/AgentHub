//! Official login by spawning the agent's CLI (`kiro-cli login`), then importing.

use std::collections::HashMap;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use crate::error::{AppError, Result};
use crate::models::AgentId;
use crate::utils::process::{
    apply_no_window, configure_process_group, poll_child, CancelToken, ChildPoll,
};

use super::pkce;
use super::session::OAuthSession;
use super::{store, StartOAuthResult};

const KIRO_LOGIN_SENTINEL: &str = "kiro-cli-login";

fn cancel_registry() -> &'static Mutex<HashMap<String, CancelToken>> {
    static REGISTRY: std::sync::OnceLock<Mutex<HashMap<String, CancelToken>>> =
        std::sync::OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn cancel_kiro_cli_login(state: &str) {
    let Ok(mut jobs) = cancel_registry().lock() else {
        return;
    };
    if let Some(token) = jobs.remove(state) {
        token.cancel();
    }
}

pub fn start_kiro_cli_login() -> Result<StartOAuthResult> {
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

    let cancel = CancelToken::new();
    {
        let mut jobs = cancel_registry()
            .lock()
            .map_err(|_| AppError::message("oauth.store", "kiro login store poisoned"))?;
        jobs.insert(state.clone(), cancel.clone());
    }

    let mut cmd = Command::new(&bin);
    cmd.args(["login", "--license", "free"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .env("TERM", "dumb")
        .env("NO_COLOR", "1")
        .env("CLICOLOR", "0");
    apply_no_window(&mut cmd);
    let process_control = configure_process_group(&mut cmd).map_err(|_| {
        let _ = st.mark_error(&state, "spawn failed");
        cancel_kiro_cli_login(&state);
        AppError::message("oauth.kiro", "无法打开 Kiro 登录")
    })?;
    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(_) => {
            let _ = st.mark_error(&state, "spawn failed");
            cancel_kiro_cli_login(&state);
            return Err(AppError::message("oauth.kiro", "无法打开 Kiro 登录"));
        }
    };
    if process_control.attach(&child).is_err() {
        let _ = child.kill();
        process_control.terminate(&mut child);
        let _ = st.mark_error(&state, "spawn failed");
        cancel_kiro_cli_login(&state);
        return Err(AppError::message("oauth.kiro", "无法打开 Kiro 登录"));
    }

    let state_for_thread = state.clone();
    thread::spawn(move || {
        watch_kiro_login(state_for_thread, before, cancel, child, process_control);
    });

    Ok(StartOAuthResult {
        state,
        authorize_url: String::new(),
        redirect_uri: String::new(),
        agent_id: AgentId::Kiro,
        provider_key: Some("kiro".into()),
        browser_opened: true,
        expires_in_secs: crate::catalog::limits::OAUTH_CALLBACK_LISTEN_TIMEOUT.as_secs(),
    })
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

fn watch_kiro_login(
    state: String,
    before: Option<String>,
    cancel: CancelToken,
    mut child: std::process::Child,
    process_control: crate::utils::process::ProcessControl,
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
    cancel_kiro_cli_login(&state);
    if !st.is_waiting(&state) {
        return;
    }
    match outcome {
        Some(true) => {
            let _ = st.set_code(&state, KIRO_LOGIN_SENTINEL.to_string());
        }
        Some(false) => {
            let _ = st.mark_error(&state, "login failed");
        }
        None => {}
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
    match accounts.import_live(AgentId::Kiro, None) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_kiro_cli_does_not_start_pkce() {
        let err = start_kiro_cli_login().unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Kiro"), "{msg}");
        assert!(!msg.contains("PKCE"), "{msg}");
        assert!(!msg.contains("start_device_oauth"), "{msg}");
    }

    #[test]
    fn cancel_unknown_session_is_idempotent() {
        cancel_kiro_cli_login("missing-kiro-login");
    }
}
