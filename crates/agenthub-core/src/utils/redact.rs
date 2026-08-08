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
    out = regex_replace_static(
        &out,
        r"(?i)(bearer\s+)[a-z0-9._\-+/=]{8,}",
        "${1}***",
    );

    // secret_key=value or "api_key": "..."
    out = regex_replace_static(
        &out,
        r#"(?i)((?:api[_-]?key|auth[_-]?token|access[_-]?token|refresh[_-]?token|client[_-]?secret|password|authorization)\s*[=:]\s*)(["']?)([^\s"',;]{4,})(["']?)"#,
        "${1}${2}***${4}",
    );

    // Common token prefixes (keep short head for support)
    for prefix in ["sk-", "xai-", "ghp_", "gho_", "github_pat_", "xoxb-", "xoxp-"] {
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

/// Recursively redact secret keys in a JSON value. Non-object/array leaves are
/// cloned as-is. Secret values become the string `"***"`.
pub fn redact_json(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut out = Map::new();
            // TOML live configs are stored losslessly as an opaque string.
            // Without a full TOML-aware secret schema, exposing any part of
            // that string could leak inline api_key/token values or secrets
            // embedded in comments. Fail closed and mask the complete body.
            let opaque_toml = map.get("format").and_then(Value::as_str) == Some("toml")
                && map.get("content").is_some_and(Value::is_string);
            for (k, v) in map {
                if is_secret_key(k) || (opaque_toml && k == "content") {
                    out.insert(k.clone(), Value::String("***".into()));
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
    fn opaque_toml_content_is_masked_completely() {
        let input = json!({
            "format": "toml",
            "content": "model = 'grok'\napi_key = 'xai-secret'\n# token: also-secret\n"
        });
        let output = redact_json(&input);
        assert_eq!(output["format"], "toml");
        assert_eq!(output["content"], "***");
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
        assert!(s.contains("https://***@github.com/org/repo.git#main"), "{s}");
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
