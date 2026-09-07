//! Best-effort account identity extraction from OAuth token responses.
//!
//! Feeds `stable_live_identity` and same-agent OAuth overwrite (email vs email,
//! subject-like ids vs each other). JWT payloads are decoded without signature
//! verification; missing fields stay fail-closed.

use serde_json::{json, Map, Value};

/// Identity fields extracted from a token endpoint response / JWT claims.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OAuthIdentity {
    pub email: Option<String>,
    pub subject: Option<String>,
    pub account_id: Option<String>,
    pub organization_id: Option<String>,
    /// Provider plan / tier (e.g. ChatGPT Plus, Grok Super).
    pub subscription: Option<String>,
}

impl OAuthIdentity {
    /// Preferred UI label: email → short subject → account_id.
    pub fn display_label(&self) -> Option<String> {
        if let Some(ref email) = self.email {
            return Some(email.clone());
        }
        if let Some(ref sub) = self.subject {
            return Some(short_id(sub));
        }
        self.account_id.as_ref().map(|s| short_id(s))
    }

    pub fn is_empty(&self) -> bool {
        self.email.is_none()
            && self.subject.is_none()
            && self.account_id.is_none()
            && self.organization_id.is_none()
            && self.subscription.is_none()
    }

    /// Merge `other` into self, filling only empty fields.
    pub fn merge_missing(&mut self, other: &OAuthIdentity) {
        if self.email.is_none() {
            self.email = other.email.clone();
        }
        if self.subject.is_none() {
            self.subject = other.subject.clone();
        }
        if self.account_id.is_none() {
            self.account_id = other.account_id.clone();
        }
        if self.organization_id.is_none() {
            self.organization_id = other.organization_id.clone();
        }
        if self.subscription.is_none() {
            self.subscription = other.subscription.clone();
        }
    }
}

/// Extract identity from a raw token JSON body and optional bearer/id tokens.
pub fn extract_oauth_identity(
    provider_id: &str,
    body: &Value,
    access_token: Option<&str>,
    id_token: Option<&str>,
) -> OAuthIdentity {
    let mut id = OAuthIdentity::default();

    // 1) Nested objects on the token response (Claude-style account/organization).
    id.merge_missing(&identity_from_token_body(body));

    // 2) JWT claims (Codex id_token / Grok access or id token).
    if let Some(tok) = id_token.filter(|s| !s.is_empty()) {
        if let Some(claims) = decode_jwt_payload(tok) {
            id.merge_missing(&identity_from_jwt_claims(&claims));
        }
    }
    if let Some(tok) = access_token.filter(|s| !s.is_empty()) {
        // Access tokens are often opaque; decode is best-effort.
        if let Some(claims) = decode_jwt_payload(tok) {
            id.merge_missing(&identity_from_jwt_claims(&claims));
        }
    }

    // 3) Provider-specific nested claim paths.
    // Pi Codex uses canonical `openai-codex` / `pi-openai-codex`, not `codex`.
    if is_openai_codex_identity_provider(provider_id) {
        for tok in [id_token, access_token].into_iter().flatten() {
            if tok.is_empty() {
                continue;
            }
            if let Some(claims) = decode_jwt_payload(tok) {
                id.merge_missing(&identity_from_openai_auth_claims(&claims));
            }
        }
    }

    id
}

/// ChatGPT / Codex OAuth identity providers, including Pi's openai-codex slot.
pub fn is_openai_codex_identity_provider(provider_id: &str) -> bool {
    let p = provider_id.trim();
    p.eq_ignore_ascii_case("codex")
        || p.eq_ignore_ascii_case("openai")
        || p.eq_ignore_ascii_case("openai-codex")
        || p.eq_ignore_ascii_case("pi-openai-codex")
}

/// Best-effort ChatGPT account id from an access/id JWT (unverified payload).
pub fn chatgpt_account_id_from_token(token: &str) -> Option<String> {
    let claims = decode_jwt_payload(token)?;
    identity_from_openai_auth_claims(&claims)
        .account_id
        .or_else(|| identity_from_jwt_claims(&claims).account_id)
}

/// Write identity into credentials map (display fields only; not secrets).
pub fn apply_identity_to_credentials(creds: &mut Map<String, Value>, identity: &OAuthIdentity) {
    if let Some(ref email) = identity.email {
        creds.insert("email".into(), json!(email));
    }
    if let Some(ref sub) = identity.subject {
        creds.insert("sub".into(), json!(sub));
    }
    if let Some(ref account_id) = identity.account_id {
        creds
            .entry("account_id".to_string())
            .or_insert_with(|| json!(account_id));
    }
    if let Some(ref org) = identity.organization_id {
        creds
            .entry("organization_id".to_string())
            .or_insert_with(|| json!(org));
        // Claude historical field name.
        creds
            .entry("org_uuid".to_string())
            .or_insert_with(|| json!(org));
    }
    if let Some(ref plan) = identity.subscription {
        creds
            .entry("plan_type".to_string())
            .or_insert_with(|| json!(plan));
    }
}

/// Build `extra` object for pool storage / UI mapping.
pub fn identity_extra(
    provider_id: &str,
    identity: &OAuthIdentity,
    expires_at: Option<&str>,
    source: &str,
) -> Value {
    let mut map = Map::new();
    map.insert("source".into(), json!(source));
    map.insert("provider".into(), json!(provider_id));
    if let Some(exp) = expires_at {
        map.insert("expiresAt".into(), json!(exp));
    }
    if let Some(ref email) = identity.email {
        map.insert("email".into(), json!(email));
    }
    if let Some(ref label) = identity.display_label() {
        map.insert("identityLabel".into(), json!(label));
    }
    if let Some(ref plan) = identity.subscription {
        map.insert("subscription".into(), json!(plan));
    } else {
        map.insert("subscription".into(), Value::Null);
    }
    if let Some(ref account_id) = identity.account_id {
        map.insert("accountId".into(), json!(account_id));
    }
    Value::Object(map)
}

/// Read identity already stored on credentials (e.g. previous refresh).
pub fn identity_from_credentials(credentials: &Value) -> OAuthIdentity {
    OAuthIdentity {
        email: string_field(credentials, &["email", "email_address", "emailAddress"]),
        subject: string_field(credentials, &["sub", "subject", "user_id", "userId"]),
        account_id: string_field(
            credentials,
            &[
                "account_id",
                "accountId",
                "account_uuid",
                "chatgpt_account_id",
            ],
        ),
        organization_id: string_field(
            credentials,
            &["organization_id", "organizationId", "org_uuid", "orgUUID"],
        ),
        subscription: string_field(
            credentials,
            &[
                "plan_type",
                "planType",
                "subscription",
                "subscription_tier",
                "chatgpt_plan_type",
            ],
        ),
    }
}

fn identity_from_token_body(body: &Value) -> OAuthIdentity {
    let mut id = OAuthIdentity {
        email: string_field(
            body,
            &["email", "email_address", "emailAddress", "user_email"],
        ),
        subject: string_field(body, &["sub", "subject", "user_id", "userId"]),
        account_id: string_field(body, &["account_id", "accountId", "account_uuid"]),
        organization_id: string_field(
            body,
            &["organization_id", "organizationId", "org_uuid", "orgUUID"],
        ),
        subscription: string_field(
            body,
            &["plan_type", "planType", "subscription", "subscription_tier"],
        ),
    };

    // Claude: { "account": { "uuid", "email_address" }, "organization": { "uuid" } }
    if let Some(account) = body.get("account") {
        if id.email.is_none() {
            id.email = string_field(account, &["email_address", "email", "emailAddress"]);
        }
        if id.account_id.is_none() {
            id.account_id = string_field(account, &["uuid", "id", "account_uuid"]);
        }
    }
    if let Some(org) = body.get("organization") {
        if id.organization_id.is_none() {
            id.organization_id = string_field(org, &["uuid", "id"]);
        }
    }

    // Codex live auth.json style: { "account": { "email": "..." } }
    if let Some(account) = body.get("account") {
        if id.email.is_none() {
            id.email = string_field(account, &["email"]);
        }
    }

    id
}

fn identity_from_jwt_claims(claims: &Value) -> OAuthIdentity {
    let mut id = OAuthIdentity {
        email: string_field(claims, &["email", "email_address", "preferred_username"]),
        subject: string_field(
            claims,
            &["sub", "user_id", "uid", "principal_id", "principalId"],
        ),
        account_id: string_field(
            claims,
            &["account_id", "chatgpt_account_id", "aid", "team_id"],
        ),
        organization_id: string_field(claims, &["org_id", "organization_id", "poid", "tid"]),
        subscription: string_field(
            claims,
            &[
                "plan_type",
                "chatgpt_plan_type",
                "subscription_tier",
                "subscription",
            ],
        ),
    };
    // xAI puts numeric `tier` on access tokens (no email claim even with email scope).
    if id.subscription.is_none() {
        if let Some(tier) = claims.get("tier") {
            if let Some(n) = tier.as_i64() {
                id.subscription = Some(format!("tier {n}"));
            } else if let Some(s) = tier.as_str().map(str::trim).filter(|s| !s.is_empty()) {
                id.subscription = Some(format!("tier {s}"));
            }
        }
    }
    id
}

/// OpenAI nests plan/account under `https://api.openai.com/auth`.
fn identity_from_openai_auth_claims(claims: &Value) -> OAuthIdentity {
    let mut id = OAuthIdentity::default();
    id.email = string_field(claims, &["email"]);

    let auth = claims
        .get("https://api.openai.com/auth")
        .or_else(|| claims.get("https://auth.openai.com/auth"));
    let Some(auth) = auth else {
        return id;
    };

    if id.account_id.is_none() {
        id.account_id = string_field(
            auth,
            &["chatgpt_account_id", "account_id", "chatgpt_user_id"],
        );
    }
    if id.subject.is_none() {
        id.subject = string_field(auth, &["user_id", "chatgpt_user_id"]);
    }
    if id.subscription.is_none() {
        id.subscription = string_field(auth, &["chatgpt_plan_type", "plan_type"]);
    }
    if id.organization_id.is_none() {
        id.organization_id = string_field(auth, &["poid", "organization_id"]);
        if id.organization_id.is_none() {
            if let Some(orgs) = auth.get("organizations").and_then(|v| v.as_array()) {
                // Prefer default org, else first.
                let default = orgs.iter().find(|o| {
                    o.get("is_default")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false)
                });
                let pick = default.or_else(|| orgs.first());
                if let Some(o) = pick {
                    id.organization_id = string_field(o, &["id"]);
                }
            }
        }
    }
    id
}

/// Decode JWT payload without verifying the signature.
/// Unverified claims feed identity extraction; missing payload stays fail-closed.
pub fn decode_jwt_payload(token: &str) -> Option<Value> {
    // Some providers return `header.payload.signature` with URL-safe base64 and
    // optional padding. Reject only when we cannot recover a JSON payload.
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() < 2 {
        return None;
    }
    let payload = parts[1];
    let decoded = base64url_decode(payload)?;
    serde_json::from_slice(&decoded).ok()
}

fn base64url_decode(input: &str) -> Option<Vec<u8>> {
    use base64::Engine;
    // Try unpadded URL_SAFE first (common for OIDC), then padded STANDARD after
    // alphabet normalization.
    if let Ok(bytes) = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(input) {
        return Some(bytes);
    }
    let mut s = input.replace('-', "+").replace('_', "/");
    match s.len() % 4 {
        2 => s.push_str("=="),
        3 => s.push_str("="),
        0 => {}
        _ => return None,
    }
    base64::engine::general_purpose::STANDARD
        .decode(s.as_bytes())
        .ok()
        .or_else(|| {
            base64::engine::general_purpose::URL_SAFE
                .decode(s.as_bytes())
                .ok()
        })
}

fn string_field(v: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(s) = v
            .get(*key)
            .and_then(|x| x.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            return Some(s.to_string());
        }
    }
    None
}

/// Shorten UUIDs / long ids for UI labels (keep full value in credentials).
fn short_id(raw: &str) -> String {
    let t = raw.trim();
    if t.len() > 12 && t.contains('-') {
        // UUID-like → first segment
        if let Some(head) = t.split('-').next() {
            if head.len() >= 8 {
                return format!("{head}…");
            }
        }
    }
    if t.len() > 16 {
        format!("{}…", &t[..12])
    } else {
        t.to_string()
    }
}

#[cfg(test)]
mod tests;
