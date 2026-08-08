//! Loopback HTTP callback listener + open browser.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::time::Duration;

use crate::error::{AppError, Result};
use crate::logging::targets;
use crate::utils::redact::redact_text;

use super::session::SessionStore;

/// Bind fixed port when provided, otherwise ephemeral on 127.0.0.1.
pub fn bind_listener(preferred_port: Option<u16>) -> Result<TcpListener> {
    let addr = match preferred_port {
        Some(p) => SocketAddr::from(([127, 0, 0, 1], p)),
        None => SocketAddr::from(([127, 0, 0, 1], 0)),
    };
    let listener = TcpListener::bind(addr).map_err(|e| {
        AppError::message(
            "oauth.bind",
            format!("无法绑定 OAuth 回调端口 {}: {e}", addr),
        )
    })?;
    listener
        .set_nonblocking(false)
        .map_err(|e| AppError::message("oauth.bind", e.to_string()))?;
    // Accept timeout via set_read_timeout on stream; listener accept uses OS default.
    Ok(listener)
}

/// Block until one HTTP request is handled for this state (or error).
pub fn spawn_callback_listener(
    listener: TcpListener,
    store: Arc<SessionStore>,
    expected_state: &str,
) -> Result<()> {
    // Overall wait is controlled by wait_oauth; here accept with long timeout by polling.
    listener
        .set_nonblocking(true)
        .map_err(|e| AppError::message("oauth.accept", e.to_string()))?;

    let deadline = std::time::Instant::now() + crate::catalog::limits::OAUTH_CALLBACK_LISTEN_TIMEOUT;
    loop {
        if std::time::Instant::now() >= deadline {
            let _ = store.mark_error(expected_state, "callback listener timed out");
            return Err(AppError::message("oauth.timeout", "callback listener timed out"));
        }
        match listener.accept() {
            Ok((stream, _)) => {
                if let Err(e) = handle_connection(stream, &store, expected_state) {
                    let err_msg = redact_text(&e.to_string());
                    tracing::warn!(
                        module = targets::OAUTH,
                        code = e.code(),
                        op = "handle",
                        error = %err_msg,
                        "oauth http handle failed"
                    );
                    let _ = store.mark_error(expected_state, err_msg);
                }
                return Ok(());
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => {
                let _ = store.mark_error(expected_state, e.to_string());
                return Err(AppError::message("oauth.accept", e.to_string()));
            }
        }
    }
}

fn handle_connection(
    mut stream: TcpStream,
    store: &SessionStore,
    expected_state: &str,
) -> Result<()> {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    let mut buf = [0u8; 8192];
    let n = stream.read(&mut buf).unwrap_or(0);
    let req = String::from_utf8_lossy(&buf[..n]);
    let first = req.lines().next().unwrap_or("");
    // GET /callback?code=...&state=... HTTP/1.1
    let path = first.split_whitespace().nth(1).unwrap_or("/");
    let q = path.split('?').nth(1).unwrap_or("");
    let params = parse_query(q);
    let state = params.get("state").map(String::as_str).unwrap_or("");
    let code = params.get("code").cloned();
    let err = params.get("error").cloned();

    if state != expected_state {
        let body = html_page("OAuth state 不匹配", "请关闭此页并在 AgentHub 重试。");
        write_response(&mut stream, 400, &body)?;
        return Err(AppError::message("oauth.state", "OAuth state mismatch"));
    }
    if let Some(e) = err {
        let body = html_page("授权失败", &e);
        write_response(&mut stream, 400, &body)?;
        store.mark_error(expected_state, e)?;
        return Ok(());
    }
    let Some(code) = code else {
        let body = html_page("缺少 code", "回调未包含授权码。");
        write_response(&mut stream, 400, &body)?;
        store.mark_error(expected_state, "missing code")?;
        return Ok(());
    };

    store.set_code(expected_state, code)?;
    let body = html_page(
        "授权成功",
        "可以关闭此窗口，返回 AgentHub 继续。",
    );
    write_response(&mut stream, 200, &body)?;
    Ok(())
}

fn parse_query(q: &str) -> std::collections::HashMap<String, String> {
    let mut m = std::collections::HashMap::new();
    for pair in q.split('&') {
        if pair.is_empty() {
            continue;
        }
        let mut it = pair.splitn(2, '=');
        let k = it.next().unwrap_or("");
        let v = it.next().unwrap_or("");
        m.insert(
            urlencoding::decode(k).unwrap_or_default().into_owned(),
            urlencoding::decode(v).unwrap_or_default().into_owned(),
        );
    }
    m
}

fn write_response(stream: &mut TcpStream, status: u16, body: &str) -> Result<()> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        _ => "Error",
    };
    let resp = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(resp.as_bytes())
        .map_err(|e| AppError::message("oauth.http", e.to_string()))?;
    Ok(())
}

fn html_page(title: &str, msg: &str) -> String {
    format!(
        r#"<!DOCTYPE html><html><head><meta charset="utf-8"><title>{title}</title></head>
<body style="font-family:system-ui;padding:2rem;text-align:center">
<h2>{title}</h2><p>{msg}</p>
<script>setTimeout(function(){{ window.close(); }}, 1200);</script>
</body></html>"#
    )
}

pub fn open_in_browser(url: &str) -> Result<()> {
    let url = url.trim();
    if url.is_empty() {
        return Err(AppError::message("oauth.browser", "url is empty"));
    }

    #[cfg(target_os = "windows")]
    {
        // `cmd /C start "" <url>` breaks on `&` `?` `#` in query/fragment unless carefully
        // quoted. `rundll32 url.dll,FileProtocolHandler` treats the URL as one arg and is
        // the reliable system-browser opener on Windows (incl. Tauri GUI).
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let status = std::process::Command::new("rundll32")
            .args(["url.dll,FileProtocolHandler", url])
            .creation_flags(CREATE_NO_WINDOW)
            .status()
            .map_err(|e| AppError::message("oauth.browser", e.to_string()))?;
        if status.success() {
            return Ok(());
        }
        // Fallback: quoted `start` title + URL.
        let quoted = format!("\"{}\"", url.replace('"', ""));
        std::process::Command::new("cmd")
            .args(["/C", "start", "", &quoted])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| AppError::message("oauth.browser", e.to_string()))?;
        return Ok(());
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(url)
            .spawn()
            .map_err(|e| AppError::message("oauth.browser", e.to_string()))?;
        return Ok(());
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(url)
            .spawn()
            .map_err(|e| AppError::message("oauth.browser", e.to_string()))?;
        return Ok(());
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", unix)))]
    {
        let _ = url;
        Err(AppError::Unsupported(
            "open browser is not supported on this platform".into(),
        ))
    }
}
