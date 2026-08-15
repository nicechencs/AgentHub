//! Heal missing / placeholder identity fields from stored credentials (pre-redaction).
//!
//! Live imports often store only `{ format, body }` with native agent auth shapes
//! (Codex tokens, Grok profile map, Pi multi-provider, Kimi JWT). After redaction
//! the frontend cannot decode secrets — identity must live in `extra` / `label`.

use serde_json::{json, Value};

use crate::adapters::pi_auth;
use crate::models::{Account, AgentId};
use crate::oauth::{
    apply_identity_to_credentials, extract_oauth_identity, identity_from_credentials, OAuthIdentity,
};

/// Whether the account still needs identity enrichment for UI.
pub fn needs_identity_heal(account: &Account) -> bool {
    let extra = &account.extra;
    let email = extra
        .get("email")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let id_label = extra
        .get("identityLabel")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .unwrap_or("");
    let label = account.label.trim();

    // Placeholder title must always be upgraded when possible.
    if is_placeholder_label(label, account.agent_id) {
        return true;
    }
    // Have email in extra but title still not the email.
    if let Some(e) = email {
        if label != e && !label.contains(e) {
            return true;
        }
        return false;
    }
    // identityLabel is account-id UUID / placeholder, not a real email.
    if id_label.is_empty()
        || is_placeholder_label(id_label, account.agent_id)
        || looks_like_uuid(id_label)
        || id_label == label
    {
        return true;
    }
    false
}

/// Best-effort extract identity from credentials; mutates account when improved.
/// Returns true if the account was modified (caller should persist).
pub fn heal_account_identity(account: &mut Account) -> bool {
    // Codex: promote legacy PKCE token bundles (`type=oauth`, no format) into
    // live-writable `auth_json` before identity/token extraction.
    let shape_dirty = heal_codex_credential_shape(account);

    let Some(identity) = extract_identity_from_credentials(account.agent_id, &account.credentials)
    else {
        // Still try to upgrade pure placeholder labels using nothing but body provider.
        let mut dirty = shape_dirty || upgrade_placeholder_label_only(account);
        if super::account_quota::heal_token_expiry(account) {
            dirty = true;
        }
        return dirty;
    };
    if identity.is_empty() {
        let mut dirty = shape_dirty || upgrade_placeholder_label_only(account);
        if super::account_quota::heal_token_expiry(account) {
            dirty = true;
        }
        return dirty;
    }

    let before = (
        account.label.clone(),
        account.extra.clone(),
        account.credentials.clone(),
    );

    // Flatten identity into credentials for authorization_key / refresh helpers.
    if let Some(obj) = account.credentials.as_object_mut() {
        apply_identity_to_credentials(obj, &identity);
        flatten_tokens_for_agent(account.agent_id, obj, &before.2);
    }

    // Ensure extra is an object.
    if !account.extra.is_object() {
        account.extra = json!({});
    }

    if let Some(obj) = account.extra.as_object_mut() {
        // Prefer email for display identity; never leave UUID account_id as the only identity
        // when email is available.
        if let Some(ref email) = identity.email {
            obj.insert("email".into(), json!(email));
            obj.insert("identityLabel".into(), json!(email));
        } else if let Some(lab) = identity.display_label() {
            let prev = obj
                .get("identityLabel")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if prev.is_empty()
                || is_placeholder_label(prev, account.agent_id)
                || looks_like_uuid(prev)
                || prev == account.label
            {
                obj.insert("identityLabel".into(), json!(lab));
            }
        }
        if let Some(ref plan) = identity.subscription {
            obj.insert("subscription".into(), json!(plan));
        }
        if let Some(ref sub) = identity.subject {
            obj.insert("sub".into(), json!(sub));
        }
        if let Some(ref aid) = identity.account_id {
            obj.insert("accountId".into(), json!(aid));
        }
        if account.agent_id == AgentId::Pi {
            if let Some(p) = pi_provider_key(&account.credentials) {
                obj.insert("provider".into(), json!(p));
            }
        }
        // expiresAt for token remaining bar
        if obj.get("expiresAt").and_then(|v| v.as_str()).is_none() {
            if let Some(exp) = account
                .credentials
                .get("expires_at")
                .and_then(|v| v.as_str())
            {
                obj.insert("expiresAt".into(), json!(exp));
            }
        }
    }

    // Always try JWT / nested expires promotion (Codex often has no expires_at field).
    let _ = super::account_quota::heal_token_expiry(account);

    // Upgrade label to best display value when current is weak or not matching email.
    if let Some(best) = best_label_for(account.agent_id, &identity, &account.credentials) {
        let cur = account.label.trim();
        if is_placeholder_label(cur, account.agent_id)
            || identity
                .email
                .as_ref()
                .map(|e| cur != e && !cur.contains(e.as_str()))
                .unwrap_or(false)
        {
            account.label = best;
        }
    }

    shape_dirty
        || before
            != (
                account.label.clone(),
                account.extra.clone(),
                account.credentials.clone(),
            )
}

/// Convert Codex OAuth PKCE token bundles into `format=auth_json` when needed.
fn heal_codex_credential_shape(account: &mut Account) -> bool {
    if account.agent_id != AgentId::Codex || account.kind != crate::models::AccountKind::Oauth {
        return false;
    }
    let format = account
        .credentials
        .get("format")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let has_live_body = account
        .credentials
        .pointer("/body/tokens/access_token")
        .and_then(|v| v.as_str())
        .is_some_and(|s| !s.is_empty())
        || account
            .credentials
            .pointer("/body/tokens/refresh_token")
            .and_then(|v| v.as_str())
            .is_some_and(|s| !s.is_empty());
    if format == "auth_json" && has_live_body {
        return false;
    }
    match crate::adapters::normalize_codex_oauth_credentials(&account.credentials) {
        Ok(normalized) if normalized != account.credentials => {
            account.credentials = normalized;
            true
        }
        _ => false,
    }
}

fn upgrade_placeholder_label_only(account: &mut Account) -> bool {
    if !is_placeholder_label(&account.label, account.agent_id) {
        return false;
    }
    if account.agent_id == AgentId::Pi {
        if let Some(p) = pi_provider_key(&account.credentials) {
            let next = format!("pi:{p}");
            if account.label != next {
                account.label = next;
                return true;
            }
        }
    }
    false
}

fn best_label_for(agent: AgentId, identity: &OAuthIdentity, credentials: &Value) -> Option<String> {
    if let Some(ref email) = identity.email {
        return Some(email.clone());
    }
    let core = identity.display_label()?;
    if agent == AgentId::Pi {
        if let Some(p) = pi_provider_key(credentials) {
            return Some(format!("pi:{p} · {core}"));
        }
    }
    Some(core)
}

fn flatten_tokens_for_agent(
    agent: AgentId,
    obj: &mut serde_json::Map<String, Value>,
    original: &Value,
) {
    // Codex: body.tokens.*
    if let Some(tokens) = original.pointer("/body/tokens") {
        if obj.get("access_token").and_then(|v| v.as_str()).is_none() {
            if let Some(a) = tokens.get("access_token").and_then(|v| v.as_str()) {
                obj.insert("access_token".into(), json!(a));
            }
        }
        if obj.get("refresh_token").and_then(|v| v.as_str()).is_none() {
            if let Some(r) = tokens.get("refresh_token").and_then(|v| v.as_str()) {
                obj.insert("refresh_token".into(), json!(r));
            }
        }
        if obj.get("id_token").and_then(|v| v.as_str()).is_none() {
            if let Some(i) = tokens.get("id_token").and_then(|v| v.as_str()) {
                obj.insert("id_token".into(), json!(i));
            }
        }
    }
    // Kimi: body is the token object
    if agent == AgentId::Kimi {
        if let Some(body) = original.get("body") {
            if obj.get("access_token").and_then(|v| v.as_str()).is_none() {
                if let Some(a) = body.get("access_token").and_then(|v| v.as_str()) {
                    obj.insert("access_token".into(), json!(a));
                }
            }
            if obj.get("refresh_token").and_then(|v| v.as_str()).is_none() {
                if let Some(r) = body.get("refresh_token").and_then(|v| v.as_str()) {
                    obj.insert("refresh_token".into(), json!(r));
                }
            }
            if let Some(exp) = body.get("expires_at") {
                if obj.get("expires_at").is_none() {
                    // Kimi often stores unix seconds as number.
                    if let Some(n) = exp.as_i64() {
                        if let Some(dt) = chrono::DateTime::from_timestamp(n, 0) {
                            obj.insert("expires_at".into(), json!(dt.to_rfc3339()));
                        }
                    } else if let Some(s) = exp.as_str() {
                        obj.insert("expires_at".into(), json!(s));
                    }
                }
            }
        }
    }
    // Pi provider entry
    if agent == AgentId::Pi {
        if let Some(p) = pi_provider_key(original) {
            obj.entry("provider".to_string())
                .or_insert_with(|| json!(p));
            if let Some(entry) = original.get("body").and_then(|b| b.get(&p)) {
                if obj.get("access_token").and_then(|v| v.as_str()).is_none() {
                    if let Some(a) = entry
                        .get("access")
                        .or_else(|| entry.get("access_token"))
                        .and_then(|v| v.as_str())
                    {
                        obj.insert("access_token".into(), json!(a));
                    }
                }
                if obj.get("refresh_token").and_then(|v| v.as_str()).is_none() {
                    if let Some(r) = entry
                        .get("refresh")
                        .or_else(|| entry.get("refresh_token"))
                        .and_then(|v| v.as_str())
                    {
                        obj.insert("refresh_token".into(), json!(r));
                    }
                }
                if obj.get("expires_at").and_then(|v| v.as_str()).is_none() {
                    if let Some(ms) = entry.get("expires").and_then(|v| v.as_i64()) {
                        if let Some(dt) = chrono::DateTime::from_timestamp(ms / 1000, 0) {
                            obj.insert("expires_at".into(), json!(dt.to_rfc3339()));
                        }
                    }
                }
            }
        }
    }
}

fn extract_identity_from_credentials(agent: AgentId, credentials: &Value) -> Option<OAuthIdentity> {
    let mut id = identity_from_credentials(credentials);
    let access = credentials
        .get("access_token")
        .or_else(|| credentials.get("access"))
        .and_then(|v| v.as_str());
    let id_token = credentials.get("id_token").and_then(|v| v.as_str());
    let provider = credentials
        .get("provider")
        .and_then(|v| v.as_str())
        .unwrap_or(agent.as_str());
    id.merge_missing(&extract_oauth_identity(
        provider,
        credentials,
        access,
        id_token,
    ));

    if let Some(body) = credentials.get("body") {
        id.merge_missing(&identity_from_live_body(agent, body));
    }

    if id.email.is_none() {
        if let Some(email) = credentials
            .pointer("/body/account/email")
            .or_else(|| credentials.pointer("/account/email"))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            id.email = Some(email.to_string());
        }
    }

    if id.is_empty() {
        None
    } else {
        Some(id)
    }
}

/// Extract identity from agent-native live auth file shapes stored in credentials.body.
fn identity_from_live_body(agent: AgentId, body: &Value) -> OAuthIdentity {
    let mut id = OAuthIdentity::default();

    // Codex ~/.codex/auth.json → body.tokens.{id_token,access_token,...}
    if let Some(tokens) = body.get("tokens") {
        let access = tokens.get("access_token").and_then(|v| v.as_str());
        let id_token = tokens.get("id_token").and_then(|v| v.as_str());
        // Prefer id_token first (has email); extract_oauth_identity already merges both.
        id.merge_missing(&extract_oauth_identity("codex", tokens, access, id_token));
        // Explicit: if id_token present, force email extraction path again with id_token as primary.
        if let Some(idt) = id_token {
            id.merge_missing(&extract_oauth_identity("codex", tokens, None, Some(idt)));
        }
        if id.account_id.is_none() {
            if let Some(aid) = tokens
                .get("account_id")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                id.account_id = Some(aid.to_string());
            }
        }
    }
    if let Some(email) = body
        .pointer("/account/email")
        .or_else(|| body.get("email"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        id.email = Some(email.to_string());
    }

    // Kimi: body is the token object.
    if body.get("access_token").is_some() && body.get("tokens").is_none() {
        let access = body.get("access_token").and_then(|v| v.as_str());
        let id_token = body.get("id_token").and_then(|v| v.as_str());
        id.merge_missing(&extract_oauth_identity("kimi", body, access, id_token));
    }

    // Grok auth.json: { "https://auth.x.ai::clientId": { email, user_id, refresh_token, ... } }
    if let Some(obj) = body.as_object() {
        for (k, entry) in obj {
            if !entry.is_object() {
                continue;
            }
            let looks_profile = entry.get("email").is_some()
                || entry.get("user_id").is_some()
                || entry.get("refresh_token").is_some()
                || k.contains("auth.x.ai")
                || k == "xai"
                || k == "anthropic"
                || k == "openai-codex";
            if !looks_profile {
                continue;
            }
            let access = entry
                .get("access")
                .or_else(|| entry.get("access_token"))
                .or_else(|| entry.get("key"))
                .and_then(|v| v.as_str());
            let id_token = entry.get("id_token").and_then(|v| v.as_str());
            id.merge_missing(&extract_oauth_identity(
                if k.contains("x.ai") || k == "xai" {
                    "xai"
                } else {
                    "oauth"
                },
                entry,
                access,
                id_token,
            ));
            id.merge_missing(&identity_from_credentials(entry));
            if id.email.is_none() {
                if let Some(email) = entry
                    .get("email")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                {
                    id.email = Some(email.to_string());
                }
            }
            if id.subject.is_none() {
                if let Some(uid) = entry
                    .get("user_id")
                    .or_else(|| entry.get("principal_id"))
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                {
                    id.subject = Some(uid.to_string());
                }
            }
            // Grok profile often has first_name — keep as weak label only if nothing else.
            if id.email.is_none() && id.subject.is_none() {
                if let Some(name) = entry.get("first_name").and_then(|v| v.as_str()) {
                    id.subject = Some(name.to_string());
                }
            }
        }
    }

    // Pi multi-provider map.
    if agent == AgentId::Pi || body.get("xai").is_some() || body.get("anthropic").is_some() {
        if let Some(obj) = body.as_object() {
            let key = obj
                .keys()
                .find(|k| pi_auth::PI_OAUTH_PROVIDER_KEYS.contains(&k.as_str()))
                .cloned()
                .or_else(|| {
                    if obj.len() == 1 {
                        obj.keys().next().cloned()
                    } else {
                        None
                    }
                });
            if let Some(provider_key) = key {
                if let Some(entry) = obj.get(&provider_key) {
                    id.merge_missing(&identity_from_pi_entry(&provider_key, entry));
                }
            }
        }
    }

    if id.is_empty() {
        walk_for_identity(body, &mut id, 0);
    }

    id
}

fn identity_from_pi_entry(provider: &str, entry: &Value) -> OAuthIdentity {
    let access = entry
        .get("access")
        .or_else(|| entry.get("access_token"))
        .and_then(|v| v.as_str());
    let id_token = entry.get("id_token").and_then(|v| v.as_str());
    let mut id = extract_oauth_identity(provider, entry, access, id_token);
    id.merge_missing(&identity_from_credentials(entry));
    id
}

fn walk_for_identity(v: &Value, id: &mut OAuthIdentity, depth: u8) {
    if depth > 3 || id.email.is_some() {
        return;
    }
    match v {
        Value::Object(map) => {
            if let Some(email) = map
                .get("email")
                .and_then(|x| x.as_str())
                .map(str::trim)
                .filter(|s| s.contains('@'))
            {
                id.email = Some(email.to_string());
            }
            let access = map
                .get("access_token")
                .or_else(|| map.get("access"))
                .and_then(|x| x.as_str());
            let id_token = map.get("id_token").and_then(|x| x.as_str());
            if access.is_some() || id_token.is_some() {
                id.merge_missing(&extract_oauth_identity("oauth", v, access, id_token));
            }
            for child in map.values() {
                walk_for_identity(child, id, depth + 1);
                if id.email.is_some() {
                    break;
                }
            }
        }
        Value::Array(arr) => {
            for child in arr.iter().take(8) {
                walk_for_identity(child, id, depth + 1);
                if id.email.is_some() {
                    break;
                }
            }
        }
        _ => {}
    }
}

fn pi_provider_key(credentials: &Value) -> Option<String> {
    if let Some(p) = credentials
        .get("provider")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return Some(p.to_string());
    }
    let body = credentials.get("body")?.as_object()?;
    if body.len() == 1 {
        return body.keys().next().cloned();
    }
    pi_auth::PI_OAUTH_PROVIDER_KEYS
        .iter()
        .find(|k| body.contains_key(**k))
        .map(|s| (*s).to_string())
}

pub fn is_placeholder_label(label: &str, agent: AgentId) -> bool {
    let t = label.trim();
    if t.is_empty() {
        return true;
    }
    let lower = t.to_ascii_lowercase();
    lower.contains("(oauth)")
        || lower.ends_with(" · oauth")
        || lower.ends_with(" oauth")
        || lower.ends_with("-oauth")
        || lower == "pi-auth"
        || lower == "codex-oauth"
        || lower == "grok-oauth"
        || lower == "kimi-oauth"
        || lower == "claude-oauth"
        || lower == format!("{} · oauth", agent.display_name().to_ascii_lowercase())
        || (agent == AgentId::Pi && lower.starts_with("pi:") && lower.contains("(oauth)"))
}

fn looks_like_uuid(s: &str) -> bool {
    let t = s.trim();
    // Full UUID or shortened "fcf2a4f8…" account id style.
    if t.len() >= 32 && t.chars().filter(|c| *c == '-').count() >= 4 {
        return true;
    }
    // ChatGPT account id often stored as identityLabel by mistake.
    t.len() == 36 && t.chars().filter(|c| *c == '-').count() == 4
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::AccountKind;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    use serde_json::json;

    fn make_jwt(claims: Value) -> String {
        let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"none"}"#);
        let payload = URL_SAFE_NO_PAD.encode(claims.to_string().as_bytes());
        format!("{header}.{payload}.sig")
    }

    fn base_account(agent: AgentId, label: &str, credentials: Value, extra: Value) -> Account {
        Account {
            id: format!("{agent}-1"),
            agent_id: agent,
            kind: AccountKind::Oauth,
            label: label.into(),
            credentials,
            extra,
            status: "active".into(),
            is_current: true,
            created_at: "t".into(),
            updated_at: "t".into(),
        }
    }

    #[test]
    fn heals_codex_legacy_pkce_bundle_into_auth_json() {
        let mut acc = base_account(
            AgentId::Codex,
            "codex-oauth",
            json!({
                "type": "oauth",
                "provider": "codex",
                "access_token": "at-legacy",
                "refresh_token": "rt-legacy",
                "id_token": "idt-legacy",
                "account_id": "acc-legacy",
                "email": "legacy@example.com"
            }),
            json!({ "source": "oauth_pkce" }),
        );
        assert!(heal_account_identity(&mut acc));
        assert_eq!(
            acc.credentials.get("format").and_then(|v| v.as_str()),
            Some("auth_json")
        );
        assert_eq!(
            acc.credentials
                .pointer("/body/tokens/access_token")
                .and_then(|v| v.as_str()),
            Some("at-legacy")
        );
        assert_eq!(
            acc.credentials
                .pointer("/body/tokens/refresh_token")
                .and_then(|v| v.as_str()),
            Some("rt-legacy")
        );
        assert_eq!(
            acc.credentials.get("email").and_then(|v| v.as_str()),
            Some("legacy@example.com")
        );
    }

    #[test]
    fn heals_codex_tokens_id_token_email_and_plan() {
        let exp = chrono::Utc::now().timestamp() + 6 * 3600;
        let id_token = make_jwt(json!({
            "email": "41375197@qq.com",
            "sub": "google-oauth2|123",
            "exp": exp,
            "https://api.openai.com/auth": {
                "chatgpt_account_id": "fcf2a4f8-bbff-4598-910d-067e947e229c",
                "chatgpt_plan_type": "prolite",
                "user_id": "user-x"
            }
        }));
        let access = make_jwt(json!({ "sub": "user-x", "exp": exp }));
        let mut acc = base_account(
            AgentId::Codex,
            "codex-oauth",
            json!({
                "format": "auth_json",
                "body": {
                    "tokens": {
                        "id_token": id_token,
                        "access_token": access,
                        "refresh_token": "rt",
                        "account_id": "fcf2a4f8-bbff-4598-910d-067e947e229c"
                    }
                }
            }),
            // Bad prior identity: account UUID mistaken for identity.
            json!({
                "source": "live",
                "identityLabel": "fcf2a4f8-bbff-4598-910d-067e947e229c"
            }),
        );
        assert!(needs_identity_heal(&acc));
        assert!(heal_account_identity(&mut acc));
        assert_eq!(acc.label, "41375197@qq.com");
        assert_eq!(
            acc.extra.get("email").and_then(|v| v.as_str()),
            Some("41375197@qq.com")
        );
        assert_eq!(
            acc.extra.get("identityLabel").and_then(|v| v.as_str()),
            Some("41375197@qq.com")
        );
        assert_eq!(
            acc.extra.get("subscription").and_then(|v| v.as_str()),
            Some("prolite")
        );
        // JWT exp should surface as expiresAt so the UI can show remaining time.
        assert!(acc
            .extra
            .get("expiresAt")
            .and_then(|v| v.as_str())
            .is_some());
        assert_eq!(
            acc.extra.get("tokenExpired").and_then(|v| v.as_bool()),
            Some(false)
        );
        assert!(!needs_identity_heal(&acc));
    }

    #[test]
    fn upgrades_grok_placeholder_label_when_email_already_in_extra() {
        let mut acc = base_account(
            AgentId::Grok,
            "grok-oauth",
            json!({
                "format": "auth_json",
                "body": {
                    "https://auth.x.ai::client": {
                        "email": "user@example.com",
                        "user_id": "u-1",
                        "refresh_token": "rt"
                    }
                }
            }),
            json!({
                "source": "live",
                "email": "user@example.com",
                "identityLabel": "user@example.com"
            }),
        );
        // Even with email present, placeholder label must be upgraded.
        assert!(needs_identity_heal(&acc));
        assert!(heal_account_identity(&mut acc));
        assert_eq!(acc.label, "user@example.com");
        assert!(!needs_identity_heal(&acc));
    }

    #[test]
    fn heals_pi_auth_json_blob_from_xai_access_jwt() {
        let access = make_jwt(json!({
            "sub": "36b45542-a4c3-4a5d-b4d9-1c685d10dcd9",
            "tier": 5,
            "team_id": "team-1"
        }));
        let mut acc = base_account(
            AgentId::Pi,
            "pi:xai (oauth)",
            json!({
                "format": "auth_json",
                "body": {
                    "xai": {
                        "type": "oauth",
                        "access": access,
                        "refresh": "rt",
                        "expires": 1785682457104i64
                    }
                }
            }),
            json!({"source":"live","identityLabel":"pi:xai (oauth)"}),
        );
        assert!(heal_account_identity(&mut acc));
        assert!(acc.label.contains("xai"));
        assert_eq!(
            acc.extra.get("subscription").and_then(|v| v.as_str()),
            Some("tier 5")
        );
        assert_eq!(
            acc.extra.get("provider").and_then(|v| v.as_str()),
            Some("xai")
        );
    }
}
