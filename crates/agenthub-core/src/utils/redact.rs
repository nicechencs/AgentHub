//! Recursive JSON redaction for likely secret keys.
//!
//! Used before serializing provider (and later account) payloads to CLI/GUI.

use serde_json::{Map, Value};

/// Keys treated as secrets (matched case-insensitively).
const SECRET_KEYS: &[&str] = &[
    "api_key",
    "apikey",
    "token",
    "auth_token",
    "authtoken",
    "access_token",
    "accesstoken",
    "refresh_token",
    "refreshtoken",
    "id_token",
    "idtoken",
    "session_token",
    "sessiontoken",
    "authorization",
    "password",
    "client_secret",
    "clientsecret",
    "private_key",
    "privatekey",
];

/// Whether a JSON object key should be redacted.
pub fn is_secret_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    SECRET_KEYS.contains(&lower.as_str())
        || lower.ends_with("_api_key")
        || lower.ends_with("_auth_token")
        || lower.ends_with("_access_token")
        || lower.ends_with("_refresh_token")
        || lower.ends_with("_id_token")
        || lower.ends_with("_session_token")
        || lower.ends_with("_password")
        || lower.ends_with("_client_secret")
        || lower.ends_with("_private_key")
}

/// Mask a secret for display labels (never returns the full value).
///
/// Examples: `sk-abc…xyz9` → `sk--••••xyz9` style preview with head/tail only.
pub fn mask_secret_preview(secret: &str) -> String {
    let t = secret.trim();
    if t.is_empty() {
        return "••••".into();
    }
    let chars: Vec<char> = t.chars().collect();
    if chars.len() <= 8 {
        let head: String = chars.iter().take(2).collect();
        return format!("{head}••••");
    }
    let head: String = chars.iter().take(3).collect();
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("{head}-••••{tail}")
}

/// Last four characters of a secret, prefixed with `**`. None when too short
/// to show a tail without leaking most of the value.
pub fn mask_secret_tail(secret: &str) -> Option<String> {
    let t = secret.trim();
    if t.is_empty() || t == "***" {
        return None;
    }
    let chars: Vec<char> = t.chars().collect();
    if chars.len() < 8 {
        return None;
    }
    let tail: String = chars[chars.len() - 4..].iter().collect();
    Some(format!("**{tail}"))
}

fn is_refresh_token_key(key: &str) -> bool {
    matches!(
        key.to_ascii_lowercase().as_str(),
        "refresh_token" | "refreshtoken"
    )
}

fn find_refresh_token(value: &Value) -> Option<&str> {
    match value {
        Value::Object(map) => {
            for key in ["refresh_token", "refreshToken"] {
                if let Some(secret) = map
                    .get(key)
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty() && *s != "***")
                {
                    return Some(secret);
                }
            }
            if let Some(found) = map.get("tokens").and_then(find_refresh_token) {
                return Some(found);
            }
            if let Some(found) = map.get("body").and_then(find_refresh_token) {
                return Some(found);
            }
            for (key, child) in map {
                if is_refresh_token_key(key) {
                    continue;
                }
                if let Some(found) = find_refresh_token(child) {
                    return Some(found);
                }
            }
            None
        }
        Value::Array(items) => items.iter().find_map(find_refresh_token),
        _ => None,
    }
}

/// Head/tail preview of an OAuth refresh token. Never returns the full secret.
pub fn refresh_token_preview(credentials: &Value) -> Option<String> {
    let preview = mask_secret_preview(find_refresh_token(credentials)?);
    (preview != "••••").then_some(preview)
}

/// `**XXXX` tail of an OAuth refresh token for list chips.
pub fn refresh_token_tail(credentials: &Value) -> Option<String> {
    mask_secret_tail(find_refresh_token(credentials)?)
}

fn is_api_key_field(key: &str) -> bool {
    let lower = key.trim().to_ascii_lowercase().replace('-', "_");
    matches!(
        lower.as_str(),
        "api_key" | "apikey" | "anthropic_auth_token"
    ) || lower.ends_with("_api_key")
}

fn find_api_key(value: &Value) -> Option<&str> {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                if is_api_key_field(key) {
                    if let Some(secret) = child
                        .as_str()
                        .map(str::trim)
                        .filter(|s| !s.is_empty() && *s != "***")
                    {
                        return Some(secret);
                    }
                }
            }
            for preferred in ["auth", "env", "body"] {
                if let Some(found) = map.get(preferred).and_then(find_api_key) {
                    return Some(found);
                }
            }
            map.values().find_map(find_api_key)
        }
        Value::Array(items) => items.iter().find_map(find_api_key),
        _ => None,
    }
}

fn find_api_key_in_text(text: &str) -> Option<String> {
    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some(eq) = line.find('=') else {
            continue;
        };
        if !is_api_key_field(line[..eq].trim()) {
            continue;
        }
        let mut val = line[eq + 1..].trim();
        if let Some(rest) = val.strip_prefix('"') {
            val = rest.split('"').next().unwrap_or(rest);
        } else if let Some(rest) = val.strip_prefix('\'') {
            val = rest.split('\'').next().unwrap_or(rest);
        } else if let Some(hash) = val.find('#') {
            val = val[..hash].trim();
        }
        let val = val.trim();
        if !val.is_empty() && val != "***" {
            return Some(val.to_string());
        }
    }
    None
}

/// Full SHA-256 hex of a normalized (trimmed) secret. Identity only — never
/// log or persist the raw secret as the key.
pub fn secret_sha256_hex(secret: &str) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(secret.trim().as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Winning API key string from JSON fields or opaque TOML `content` / `config`.
pub fn api_key_secret(credentials: &Value) -> Option<String> {
    if let Some(secret) = find_api_key(credentials) {
        return Some(secret.to_string());
    }
    let content = credentials
        .get("content")
        .or_else(|| credentials.get("config"))
        .and_then(Value::as_str)?;
    find_api_key_in_text(content)
}

/// SHA-256 of the winning API key. None when no secret is present.
pub fn api_key_secret_hash(credentials: &Value) -> Option<String> {
    Some(secret_sha256_hex(&api_key_secret(credentials)?))
}

/// `**XXXX` tail of an API key for list chips (JSON fields or TOML content).
pub fn api_key_tail(credentials: &Value) -> Option<String> {
    mask_secret_tail(&api_key_secret(credentials)?)
}

/// Redact likely secrets inside free-form text (install logs, errors, chat lines).
///
/// Heuristics (fail closed on matches):
/// - URL userinfo (`https://user:token@host` → `https://***@host`)
/// - `key=value` / `key: value` for secret-like keys
/// - Bearer tokens
/// - Common API key prefixes (`sk-`, `xai-`, `ghp_`, …)
pub fn redact_text(input: &str) -> String {
    let mut out = redact_url_userinfo(input);

    // authorization: Bearer xxx / Bearer xxx
    out = regex_replace_static(&out, r"(?i)(bearer\s+)[a-z0-9._\-+/=]{8,}", "${1}***");

    // secret_key=value or "api_key": "..."
    out = regex_replace_static(
        &out,
        r#"(?i)((?:api[_-]?key|auth[_-]?token|access[_-]?token|refresh[_-]?token|client[_-]?secret|password|authorization)\s*[=:]\s*)(["']?)([^\s"',;]{4,})(["']?)"#,
        "${1}${2}***${4}",
    );

    // Common token prefixes (keep short head for support)
    for prefix in [
        "sk-",
        "xai-",
        "ghp_",
        "gho_",
        "github_pat_",
        "xoxb-",
        "xoxp-",
    ] {
        out = redact_prefixed_tokens(&out, prefix);
    }

    out
}

/// Strip URL userinfo so credentials never land in logs or error strings.
///
/// Examples:
/// - `https://user:token@github.com/org/repo.git` → `https://***@github.com/org/repo.git`
/// - `https://x-access-token:ghp_xxx@host/path` → `https://***@host/path`
/// - URLs without userinfo, and non-URL text, are unchanged.
pub fn redact_url_userinfo(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(scheme_at) = rest.find("://") {
        out.push_str(&rest[..scheme_at + 3]);
        rest = &rest[scheme_at + 3..];
        let auth_end = rest
            .find(|c: char| c == '/' || c == '?' || c == '#' || c.is_whitespace())
            .unwrap_or(rest.len());
        let authority = &rest[..auth_end];
        if let Some(at) = authority.rfind('@') {
            // Keep host[:port]; mask entire userinfo (user, password, or token).
            out.push_str("***@");
            out.push_str(&authority[at + 1..]);
        } else {
            out.push_str(authority);
        }
        rest = &rest[auth_end..];
    }
    out.push_str(rest);
    out
}

fn redact_prefixed_tokens(input: &str, prefix: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(idx) = rest.find(prefix) {
        result.push_str(&rest[..idx]);
        result.push_str(prefix);
        result.push_str("***");
        let after = &rest[idx + prefix.len()..];
        let end = after
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
            .unwrap_or(after.len());
        rest = &after[end..];
    }
    result.push_str(rest);
    result
}

/// Dispatch to hand-rolled redactors (no `regex` crate dependency).
fn regex_replace_static(input: &str, pattern: &str, _replace: &str) -> String {
    if pattern.contains("bearer") {
        return redact_bearer(input);
    }
    if pattern.contains("api[_-]?key") {
        return redact_key_assignments(input);
    }
    input.to_string()
}

fn redact_bearer(input: &str) -> String {
    // Char-safe scan (must not slice mid UTF-8).
    let mut result = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let rest: String = chars[i..].iter().collect();
        let rest_lower = rest.to_ascii_lowercase();
        if rest_lower.starts_with("bearer ") {
            result.push_str(&rest[..7]); // "bearer" + space (ASCII)
            i += 7;
            while i < chars.len() && chars[i].is_whitespace() {
                result.push(chars[i]);
                i += 1;
            }
            let start = i;
            while i < chars.len() {
                let c = chars[i];
                if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '+' | '/' | '=') {
                    i += 1;
                } else {
                    break;
                }
            }
            if i > start {
                result.push_str("***");
            }
            continue;
        }
        result.push(chars[i]);
        i += 1;
    }
    result
}

fn redact_key_assignments(input: &str) -> String {
    // Keep aligned with is_secret_key / SECRET_KEYS (assignment forms in free text).
    const KEYS: &[&str] = &[
        "api_key",
        "apikey",
        "api-key",
        "auth_token",
        "authtoken",
        "access_token",
        "refresh_token",
        "id_token",
        "session_token",
        "client_secret",
        "password",
        "private_key",
        "authorization",
    ];
    let mut out = input.to_string();
    for key in KEYS {
        out = redact_assignment(&out, key);
    }
    out
}

fn redact_assignment(input: &str, key: &str) -> String {
    let lower = input.to_ascii_lowercase();
    let key_l = key.to_ascii_lowercase();
    let mut result = String::with_capacity(input.len());
    let mut i = 0;
    let chars: Vec<char> = input.chars().collect();
    let lower_chars: Vec<char> = lower.chars().collect();
    let key_chars: Vec<char> = key_l.chars().collect();

    while i < chars.len() {
        if i + key_chars.len() <= chars.len()
            && lower_chars[i..i + key_chars.len()] == key_chars[..]
        {
            // Write original key casing
            for c in chars.iter().skip(i).take(key_chars.len()) {
                result.push(*c);
            }
            i += key_chars.len();
            // optional whitespace
            while i < chars.len() && chars[i].is_whitespace() {
                result.push(chars[i]);
                i += 1;
            }
            if i < chars.len() && (chars[i] == '=' || chars[i] == ':') {
                result.push(chars[i]);
                i += 1;
                while i < chars.len() && chars[i].is_whitespace() {
                    result.push(chars[i]);
                    i += 1;
                }
                let quote = if i < chars.len() && (chars[i] == '"' || chars[i] == '\'') {
                    let q = chars[i];
                    result.push(q);
                    i += 1;
                    Some(q)
                } else {
                    None
                };
                // skip value
                if let Some(q) = quote {
                    while i < chars.len() && chars[i] != q {
                        i += 1;
                    }
                    result.push_str("***");
                    if i < chars.len() && chars[i] == q {
                        result.push(q);
                        i += 1;
                    }
                } else {
                    while i < chars.len()
                        && !chars[i].is_whitespace()
                        && chars[i] != ','
                        && chars[i] != ';'
                        && chars[i] != '"'
                        && chars[i] != '\''
                    {
                        i += 1;
                    }
                    result.push_str("***");
                }
                continue;
            }
            continue;
        }
        result.push(chars[i]);
        i += 1;
    }
    result
}

/// Field-redact a TOML document: secret keys become `"***"`, structure stays.
/// Invalid TOML still keeps models / URLs via line masking.
pub fn redact_toml_content(content: &str) -> String {
    let stripped = strip_secret_export_lines(content);
    if let Ok(mut doc) = stripped.parse::<toml_edit::DocumentMut>() {
        redact_toml_table(doc.as_table_mut());
        let rendered = doc.to_string();
        if rendered.contains("***") || rendered.contains("api_key") {
            return mask_toml_secret_lines(&rendered);
        }
    }
    mask_toml_secret_lines(&stripped)
}

fn is_secret_export_line(line: &str) -> bool {
    let trimmed = line.trim();
    let Some(body) = trimmed
        .strip_prefix("export ")
        .or_else(|| trimmed.strip_prefix("EXPORT "))
        .or_else(|| trimmed.strip_prefix("set "))
        .or_else(|| trimmed.strip_prefix("SET "))
        .or_else(|| trimmed.strip_prefix("$env:"))
        .or_else(|| trimmed.strip_prefix("$Env:"))
    else {
        return false;
    };
    let Some((key, _)) = body.split_once('=') else {
        return false;
    };
    let key = key.trim().trim_matches(['"', '\'']);
    let upper = key.to_ascii_uppercase();
    upper.ends_with("_API_KEY")
        || upper.ends_with("_AUTH_TOKEN")
        || upper.ends_with("_TOKEN")
        || upper.ends_with("_SECRET")
        || upper == "API_KEY"
        || upper == "AUTH_TOKEN"
}

fn is_toml_secret_assignment_line(line: &str) -> bool {
    let trimmed = line.trim();
    let Some((key, _)) = trimmed.split_once('=') else {
        return false;
    };
    is_secret_key(key.trim())
}

fn strip_secret_export_lines(content: &str) -> String {
    content
        .lines()
        .filter(|line| !is_secret_export_line(line))
        .collect::<Vec<_>>()
        .join("\n")
}

fn mask_toml_secret_lines(content: &str) -> String {
    let mut out = String::with_capacity(content.len());
    for (i, line) in content.lines().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        if is_toml_secret_assignment_line(line) {
            if let Some((prefix, _)) = line.split_once('=') {
                out.push_str(prefix);
                out.push_str("= \"***\"");
                continue;
            }
        }
        out.push_str(line);
    }
    if content.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn redact_toml_item(item: &mut toml_edit::Item) {
    match item {
        toml_edit::Item::None => {}
        toml_edit::Item::Value(value) => redact_toml_value(value, false),
        toml_edit::Item::Table(table) => redact_toml_table(table),
        toml_edit::Item::ArrayOfTables(arr) => {
            for table in arr.iter_mut() {
                redact_toml_table(table);
            }
        }
    }
}

fn redact_toml_table(table: &mut toml_edit::Table) {
    let keys: Vec<String> = table.iter().map(|(key, _)| key.to_string()).collect();
    for key in keys {
        if is_secret_key(&key) {
            table[&key] = toml_edit::value("***");
            continue;
        }
        if let Some(item) = table.get_mut(&key) {
            redact_toml_item(item);
        }
    }
}

fn redact_toml_value(value: &mut toml_edit::Value, mask_all_strings: bool) {
    match value {
        toml_edit::Value::String(_) if mask_all_strings => {
            *value = toml_edit::Value::from("***");
        }
        toml_edit::Value::InlineTable(table) => {
            let keys: Vec<String> = table.iter().map(|(key, _)| key.to_string()).collect();
            for key in keys {
                if is_secret_key(&key) {
                    table.insert(&key, toml_edit::Value::from("***"));
                    continue;
                }
                if let Some(inner) = table.get_mut(&key) {
                    redact_toml_value(inner, mask_all_strings);
                }
            }
        }
        toml_edit::Value::Array(arr) => {
            for inner in arr.iter_mut() {
                redact_toml_value(inner, mask_all_strings);
            }
        }
        _ => {}
    }
}

/// Recursively redact secret keys in a JSON value. Non-object/array leaves are
/// cloned as-is. Secret values become the string `"***"`.
pub fn redact_json(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut out = Map::new();
            let toml_content = map.get("format").and_then(Value::as_str) == Some("toml")
                && map.get("content").is_some_and(Value::is_string);
            for (k, v) in map {
                if is_secret_key(k) {
                    out.insert(k.clone(), Value::String("***".into()));
                } else if toml_content && k == "content" {
                    let content = v.as_str().unwrap_or("");
                    out.insert(k.clone(), Value::String(redact_toml_content(content)));
                } else {
                    out.insert(k.clone(), redact_json(v));
                }
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(redact_json).collect()),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn redacts_known_keys_case_insensitive_and_nested() {
        let input = json!({
            "api_key": "sk-1",
            "API_KEY": "sk-2",
            "Token": "t1",
            "AUTH_TOKEN": "t2",
            "Authorization": "Bearer x",
            "base_url": "https://example.com",
            "env": {
                "auth_token": "nested",
                "ANTHROPIC_AUTH_TOKEN": "anthropic-secret",
                "OPENAI_API_KEY": "openai-secret",
                "apiKey": "camel-secret",
                "clientSecret": "oauth-secret",
                "token_count": 10,
                "safe": "ok"
            },
            "list": [
                { "token": "in-array" },
                "plain"
            ]
        });
        let out = redact_json(&input);
        assert_eq!(out["api_key"], "***");
        assert_eq!(out["API_KEY"], "***");
        assert_eq!(out["Token"], "***");
        assert_eq!(out["AUTH_TOKEN"], "***");
        assert_eq!(out["Authorization"], "***");
        assert_eq!(out["base_url"], "https://example.com");
        assert_eq!(out["env"]["auth_token"], "***");
        assert_eq!(out["env"]["ANTHROPIC_AUTH_TOKEN"], "***");
        assert_eq!(out["env"]["OPENAI_API_KEY"], "***");
        assert_eq!(out["env"]["apiKey"], "***");
        assert_eq!(out["env"]["clientSecret"], "***");
        assert_eq!(out["env"]["token_count"], 10);
        assert_eq!(out["env"]["safe"], "ok");
        assert_eq!(out["list"][0]["token"], "***");
        assert_eq!(out["list"][1], "plain");
    }

    #[test]
    fn non_object_passthrough() {
        assert_eq!(redact_json(&json!(null)), Value::Null);
        assert_eq!(redact_json(&json!(42)), json!(42));
        assert_eq!(redact_json(&json!("x")), json!("x"));
    }

    #[test]
    fn toml_content_field_redacts_secrets_and_keeps_structure() {
        let input = json!({
            "format": "toml",
            "content": "[models]\ndefault = \"grok\"\n\n[model.\"grok\"]\nmodel = \"grok-4.5\"\nbase_url = \"https://mytokens.cc/v1\"\napi_key = \"xai-secret\"\napi_backend = \"responses\"\n\n[endpoints]\napi = \"https://mytokens.cc/v1\"\n"
        });
        let output = redact_json(&input);
        assert_eq!(output["format"], "toml");
        let content = output["content"].as_str().expect("content");
        assert!(content.contains("grok-4.5"), "{content}");
        assert!(content.contains("https://mytokens.cc/v1"), "{content}");
        assert!(content.contains("api_backend"), "{content}");
        assert!(content.contains("[endpoints]"), "{content}");
        assert!(!content.contains("xai-secret"), "{content}");
        assert!(content.contains("***"), "{content}");
    }

    #[test]
    fn toml_content_strips_export_secret_lines() {
        let input = json!({
            "format": "toml",
            "content": "export XAI_API_KEY=xai-not-real\n[models]\ndefault = \"grok\"\n"
        });
        let output = redact_json(&input);
        let content = output["content"].as_str().expect("content");
        assert!(!content.contains("xai-not-real"), "{content}");
        assert!(!content.to_ascii_lowercase().contains("export xai_api_key"), "{content}");
        assert!(content.contains("[models]"), "{content}");
    }

    #[test]
    fn mask_secret_preview_hides_middle() {
        let preview = mask_secret_preview("sk-abcdefghijklmnop");
        assert!(preview.contains("••••"));
        assert!(!preview.contains("abcdefghijklmnop"));
        assert!(preview.starts_with("sk-"));
        assert_eq!(mask_secret_preview(""), "••••");
    }

    #[test]
    fn refresh_token_preview_uses_head_tail_and_nested_codex_shape() {
        let flat = json!({ "refresh_token": "rt-abcdefghijklmnopqrstuvwxyz" });
        let preview = refresh_token_preview(&flat).expect("preview");
        assert!(preview.contains("••••"));
        assert!(!preview.contains("abcdefghijklmnopqrstuvwxyz"));
        assert_eq!(
            preview,
            mask_secret_preview("rt-abcdefghijklmnopqrstuvwxyz")
        );

        let nested = json!({
            "format": "auth_json",
            "body": { "tokens": { "access_token": "at-secret", "refresh_token": "rt-nested-secret-value" } }
        });
        let nested_preview = refresh_token_preview(&nested).expect("nested preview");
        assert_eq!(
            nested_preview,
            mask_secret_preview("rt-nested-secret-value")
        );
        assert!(!nested_preview.contains("rt-nested-secret-value"));

        assert!(refresh_token_preview(&json!({ "refresh_token": "***" })).is_none());
        assert!(refresh_token_preview(&json!({ "access_token": "only-access" })).is_none());
    }

    #[test]
    fn mask_secret_tail_uses_last_four() {
        assert_eq!(
            mask_secret_tail("rt-abcdefghijklmnopqrstuvwxyz").as_deref(),
            Some("**wxyz")
        );
        assert_eq!(mask_secret_tail("short"), None);
        assert_eq!(mask_secret_tail("***"), None);
        assert_eq!(mask_secret_tail(""), None);
    }

    #[test]
    fn refresh_and_api_key_tails_read_nested_shapes() {
        let rt = json!({
            "body": { "tokens": { "refresh_token": "rt-abcdefghijklmnopqrstuvwxyz" } }
        });
        assert_eq!(refresh_token_tail(&rt).as_deref(), Some("**wxyz"));

        let json_key = json!({ "api_key": "sk-abcdefghijklmnop" });
        assert_eq!(api_key_tail(&json_key).as_deref(), Some("**mnop"));

        let claude = json!({
            "env": { "ANTHROPIC_AUTH_TOKEN": "sk-ant-abcdefghijklmnopqrstuvwxyz" }
        });
        assert_eq!(api_key_tail(&claude).as_deref(), Some("**wxyz"));

        let toml = json!({
            "format": "toml",
            "content": "model = 'grok'\napi_key = 'xai-secret-value-here'\n"
        });
        assert_eq!(api_key_tail(&toml).as_deref(), Some("**here"));
        assert!(api_key_tail(&json!({ "refresh_token": "rt-not-a-key-value" })).is_none());
    }

    #[test]
    fn secret_hash_is_stable_and_never_the_raw_secret() {
        let key = "sk-fixture-openrouter-aaaa6aa9";
        let hash = secret_sha256_hex(key);
        assert_eq!(
            hash,
            secret_sha256_hex("  sk-fixture-openrouter-aaaa6aa9\n")
        );
        assert_eq!(hash.len(), 64);
        assert!(!hash.contains(key));
        assert_ne!(hash, secret_sha256_hex("sk-fixture-openrouter-bbbb6aa9"));

        let openai_auth = json!({ "auth": { "OPENAI_API_KEY": key } });
        assert_eq!(
            api_key_secret_hash(&openai_auth).as_deref(),
            Some(hash.as_str())
        );
        assert!(api_key_secret_hash(&json!({ "api_key": "***" })).is_none());
    }

    #[test]
    fn redact_text_masks_keys_and_prefixes() {
        let s = redact_text("api_key=sk-abcdefghijklmnop and Bearer supersecrettokenvalue");
        assert!(s.contains("api_key=***") || s.contains("api_key=***"));
        assert!(s.contains("Bearer ***"));
        assert!(!s.contains("sk-abcdefghijklmnop"));
        assert!(!s.contains("supersecrettokenvalue"));
        let s2 = redact_text("token xai-abcdefghijklmnopqrst");
        assert!(s2.contains("xai-***"));
    }

    #[test]
    fn redact_text_preserves_utf8_chinese() {
        let msg = "powershell 不支持一键安装；windows 通常已自带。";
        let out = redact_text(msg);
        assert_eq!(out, msg);
        let io = "io error: 当文件已存在时，无法创建该文件。 (os error 183)";
        assert_eq!(redact_text(io), io);
    }

    #[test]
    fn is_secret_key_covers_oauth_style_keys() {
        assert!(is_secret_key("refresh_token"));
        assert!(is_secret_key("id_token"));
        assert!(is_secret_key("session_token"));
        assert!(is_secret_key("OPENAI_REFRESH_TOKEN"));
        assert!(!is_secret_key("token_count"));
        assert!(!is_secret_key("base_url"));
    }

    #[test]
    fn redact_text_masks_quoted_assignment() {
        let s = redact_text(r#"password="s3cret-value" ok"#);
        assert!(s.contains("password="));
        assert!(s.contains("***"));
        assert!(!s.contains("s3cret-value"));
    }

    #[test]
    fn redact_text_masks_oauth_and_private_key_assignments() {
        for sample in [
            "private_key=-----BEGIN-RSA-----abcdefgh",
            "session_token: abcdefghijklmnop",
            "id_token=eyJhbGciOiJIUzI1NiJ9.payload",
        ] {
            let s = redact_text(sample);
            assert!(s.contains("***"), "expected mask in {s}");
            assert!(!s.contains("abcdefgh"), "leaked value in {s}");
            assert!(!s.contains("eyJhbGciOiJIUzI1NiJ9"), "leaked jwt in {s}");
        }
    }

    #[test]
    fn redact_url_userinfo_masks_credentials_keeps_host() {
        let s = redact_url_userinfo(
            "git clone failed from https://user:s3cret-token@github.com/org/repo.git#main",
        );
        assert!(
            s.contains("https://***@github.com/org/repo.git#main"),
            "{s}"
        );
        assert!(!s.contains("s3cret-token"), "{s}");
        assert!(!s.contains("user:"), "{s}");

        let plain = "https://github.com/org/repo.git";
        assert_eq!(redact_url_userinfo(plain), plain);

        let via_text = redact_text(
            "skill.update: git clone failed for skill 'x' from https://x-access-token:ghp_abc123456789@host/p",
        );
        assert!(via_text.contains("***@host/p"), "{via_text}");
        assert!(!via_text.contains("ghp_abc123456789"), "{via_text}");
    }
}
