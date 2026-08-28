//! Shared account identity / label / authorization helpers.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use chrono::Utc;
use serde_json::{json, Value};

use crate::adapters::AgentAdapter;
use crate::error::{AppError, Result};
use crate::logging::targets;
use crate::models::{Account, AccountKind, AgentId, LiveAccount};
use crate::utils::loopback::credentials_are_loopback;
use crate::utils::redact::api_key_tail;

/// Process-local counterpart to the optional cross-process file lock. Services
/// built without a backup root have no `lock_dir`, but concurrent UI reads can
/// still reconcile the same live snapshot, so they must share this guard.
pub(super) fn live_reconcile_lock(agent: AgentId) -> Arc<Mutex<()>> {
    static LOCKS: OnceLock<Mutex<HashMap<AgentId, Arc<Mutex<()>>>>> = OnceLock::new();
    let locks = LOCKS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut locks = locks
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    Arc::clone(
        locks
            .entry(agent)
            .or_insert_with(|| Arc::new(Mutex::new(()))),
    )
}

pub(super) fn log_account_op<T>(op: &str, agent: AgentId, started: Instant, result: &Result<T>) {
    let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    match result {
        Ok(_) => {
            let msg = match op {
                "switch" => "switched account",
                "delete" => "deleted account",
                "add_api_key" => "added api key account",
                "update_api_key" => "updated api key account",
                "import" => "imported account",
                _ => "ok",
            };
            tracing::info!(
                module = targets::ACCOUNT,
                op,
                agent = agent.as_str(),
                elapsed_ms,
                "{msg}"
            );
        }
        Err(err) => {
            tracing::error!(
                module = targets::ACCOUNT,
                op,
                agent = agent.as_str(),
                code = err.code(),
                elapsed_ms,
                "account operation failed"
            );
        }
    }
}

pub(super) fn compensated_current_account_apply_error(
    primary: AppError,
    live_rollback: Option<AppError>,
) -> AppError {
    let Some(rollback) = live_rollback else {
        return primary;
    };
    AppError::message(
        "account.current.apply.rollback",
        format!(
            "applying the current account failed [{}]; compensation status: live={}",
            primary.code(),
            rollback.code()
        ),
    )
}

pub(super) fn compensated_switch_error(
    primary: AppError,
    live_rollback: Option<AppError>,
    db_rollback: Option<AppError>,
) -> AppError {
    if live_rollback.is_none() && db_rollback.is_none() {
        return primary;
    }
    let live = live_rollback.as_ref().map_or("ok", AppError::code);
    let database = db_rollback.as_ref().map_or("ok", AppError::code);
    AppError::message(
        "account.switch.rollback",
        format!(
            "account switch failed [{}]; compensation status: live={live}, database={database}",
            primary.code()
        ),
    )
}

pub(super) fn now_ts() -> String {
    Utc::now().format("%Y-%m-%d %H:%M:%S%.6f").to_string()
}

pub(super) fn read_optional_file(path: &std::path::Path) -> Result<Option<Vec<u8>>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

pub(super) fn probe_auth_revision(adapter: &dyn AgentAdapter) -> Option<String> {
    match adapter.read_auth() {
        Ok(state) => state.revision,
        Err(error) if error.code() == "not_found" || error.code() == "unsupported" => None,
        Err(error) => {
            tracing::debug!(
                module = targets::ACCOUNT,
                agent = adapter.id().as_str(),
                error_code = error.code(),
                "live auth revision probe unavailable"
            );
            None
        }
    }
}

/// Return a live account snapshot only when the adapter's opaque revision is
/// stable on both sides of the read. `None` remains a valid revision for
/// adapters without a revision probe; a transition between `Some` and `None`
/// is still treated as a conflict and retried.
pub(super) fn capture_stable_live_snapshot(
    adapter: &dyn AgentAdapter,
    attempts: usize,
) -> Result<(Option<LiveAccount>, Option<String>)> {
    for _ in 0..attempts.max(1) {
        let before = probe_auth_revision(adapter);
        let live = match adapter.read_account() {
            Ok(live) if live.agent == adapter.id() => Some(live),
            Ok(live) => {
                return Err(AppError::InvalidArg(format!(
                    "adapter returned account for {}, expected {}",
                    live.agent.as_str(),
                    adapter.id().as_str()
                )))
            }
            Err(error) if error.code() == "not_found" || error.code() == "unsupported" => None,
            Err(error) => return Err(error),
        };
        let after = probe_auth_revision(adapter);
        if before == after {
            return Ok((live, after));
        }
    }
    Err(live_revision_conflict())
}

pub(super) fn live_revision_conflict() -> AppError {
    AppError::message(
        "account.live_conflict",
        "live account changed while switching; retry the switch",
    )
}

pub(super) fn live_account_is_empty(live: &LiveAccount) -> bool {
    live.credentials
        .as_object()
        .map(|o| o.is_empty())
        .unwrap_or(true)
}

/// Pi has one live auth file with independent provider entries. Matching an
/// identity across providers is not evidence that either provider's grant may
/// be overwritten, nor that it is the UI's globally selected account.
pub(super) fn same_live_slot(agent: AgentId, incoming: &Value, existing: &Value) -> bool {
    if agent != AgentId::Pi {
        return true;
    }
    incoming
        .get("provider")
        .and_then(|value| value.as_str())
        .zip(existing.get("provider").and_then(|value| value.as_str()))
        .is_some_and(|(incoming, existing)| incoming == existing)
}

/// 是否为「同一授权票」（非身份）。见 `docs/account-authorization-pool.md`。
pub(super) fn accounts_same_authorization(
    adapter: &dyn AgentAdapter,
    kind: AccountKind,
    incoming_credentials: &Value,
    existing: &Account,
) -> bool {
    if existing.kind != kind {
        return false;
    }
    // 完整凭据相等：同 live 再 import
    if &existing.credentials == incoming_credentials {
        return true;
    }
    match (
        adapter.authorization_key(kind, incoming_credentials),
        adapter.authorization_key(kind, &existing.credentials),
    ) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

/// Same OAuth person: emails compared to emails, subject-like ids
/// (`user_id` / `sub` / `account_id` / …) compared among themselves.
/// Display labels are not identity. Unknown identity is fail-closed.
pub(super) fn accounts_same_oauth_identity(
    kind: AccountKind,
    incoming_credentials: &Value,
    existing: &Account,
) -> bool {
    if kind != AccountKind::Oauth || existing.kind != AccountKind::Oauth {
        return false;
    }
    oauth_credentials_same_identity(incoming_credentials, &existing.credentials)
}

/// Emails vs emails, subject-like ids vs subject-like ids. Empty-vs-empty is fail-closed.
pub(super) fn oauth_credentials_same_identity(left: &Value, right: &Value) -> bool {
    let left = collect_oauth_identity_marks(left);
    let right = collect_oauth_identity_marks(right);
    if left.is_empty() || right.is_empty() {
        return false;
    }
    left.intersects(&right)
}

pub(super) fn oauth_credentials_identity_unknown(value: &Value) -> bool {
    collect_oauth_identity_marks(value).is_empty()
}

const OAUTH_EMAIL_KEYS: &[&str] = &["email", "email_address", "emailAddress"];
const OAUTH_SUBJECT_KEYS: &[&str] = &[
    "user_id",
    "userId",
    "principal_id",
    "principalId",
    "sub",
    "subject",
    "account_id",
    "accountId",
    "account_uuid",
];

struct OauthIdentityMarks {
    emails: HashSet<String>,
    subjects: HashSet<String>,
}

impl OauthIdentityMarks {
    fn is_empty(&self) -> bool {
        self.emails.is_empty() && self.subjects.is_empty()
    }

    fn intersects(&self, other: &Self) -> bool {
        !self.emails.is_disjoint(&other.emails) || !self.subjects.is_disjoint(&other.subjects)
    }
}

fn collect_oauth_identity_marks(credentials: &Value) -> OauthIdentityMarks {
    let mut marks = OauthIdentityMarks {
        emails: HashSet::new(),
        subjects: HashSet::new(),
    };
    collect_oauth_identity_fields(credentials, &mut marks);
    marks
}

fn collect_oauth_identity_fields(value: &Value, marks: &mut OauthIdentityMarks) {
    match value {
        Value::Object(map) => {
            for (key, nested) in map {
                if let Some(raw) = nested.as_str().map(str::trim).filter(|s| !s.is_empty()) {
                    if OAUTH_EMAIL_KEYS.iter().any(|k| *k == key) {
                        marks.emails.insert(raw.to_ascii_lowercase());
                    } else if OAUTH_SUBJECT_KEYS.iter().any(|k| *k == key) {
                        marks.subjects.insert(raw.to_owned());
                    }
                }
            }
            for nested in map.values() {
                if nested.is_object() || nested.is_array() {
                    collect_oauth_identity_fields(nested, marks);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_oauth_identity_fields(item, marks);
            }
        }
        _ => {}
    }
}

/// Same-agent rows to upsert: token fingerprint, loopback slot, or OAuth identity.
pub(super) fn authorization_duplicates(
    adapter: &dyn AgentAdapter,
    agent: AgentId,
    kind: AccountKind,
    credentials: &Value,
    snapshot: &[Account],
) -> Vec<Account> {
    let incoming_loopback = credentials_are_loopback(credentials);
    snapshot
        .iter()
        .filter(|candidate| {
            candidate.kind == kind
                && same_live_slot(agent, credentials, &candidate.credentials)
                && if incoming_loopback {
                    credentials_are_loopback(&candidate.credentials)
                } else {
                    !credentials_are_loopback(&candidate.credentials)
                        && (accounts_same_authorization(adapter, kind, credentials, candidate)
                            || accounts_same_oauth_identity(kind, credentials, candidate))
                }
        })
        .cloned()
        .collect()
}

/// Compare the serialized live credential payload, not the authorization key.
pub(super) fn live_credentials_changed(current: &Account, live: &LiveAccount) -> bool {
    let Some(current_body) = current.credentials.get("body") else {
        return current.credentials != live.credentials;
    };
    let Some(live_body) = live.credentials.get("body") else {
        return current.credentials != live.credentials;
    };
    current.credentials.get("format") != live.credentials.get("format") || current_body != live_body
}

/// Stable identity extracted from credentials only. Passing `None` as the
/// label hint is intentional: a display label (especially a token preview)
/// is not an identity proof and must never authorize a live overwrite.
pub(super) fn stable_live_identity(
    adapter: &dyn AgentAdapter,
    kind: AccountKind,
    credentials: &Value,
) -> Option<String> {
    if let Some(identity) = adapter
        .identity_label(kind, credentials, None)
        .map(|identity| identity.trim().to_owned())
        .filter(|identity| !identity.is_empty())
    {
        return Some(identity);
    }
    // A number of file formats wrap the account object under `body`,
    // `auth`, or provider-specific maps.  Read only well-known stable
    // identity fields recursively; never use arbitrary labels/token previews.
    find_stable_identity_field(credentials)
}

pub(super) fn find_stable_identity_field(value: &Value) -> Option<String> {
    const KEYS: &[&str] = &[
        "email",
        "email_address",
        "emailAddress",
        "user_id",
        "userId",
        "principal_id",
        "principalId",
        "sub",
        "account_id",
        "accountId",
        "account_uuid",
    ];
    match value {
        Value::Object(map) => {
            for key in KEYS {
                if let Some(Value::String(identity)) = map.get(*key) {
                    let identity = identity.trim();
                    if !identity.is_empty() {
                        return Some(identity.to_owned());
                    }
                }
            }
            map.values().find_map(find_stable_identity_field)
        }
        Value::Array(items) => items.iter().find_map(find_stable_identity_field),
        _ => None,
    }
}

/// True when label is a placeholder like `Claude · OAuth` / `claude oauth`.
pub(super) fn is_generic_oauth_label(label: &str, agent: AgentId) -> bool {
    let t = label.trim();
    if t.is_empty() {
        return true;
    }
    let lower = t.to_ascii_lowercase();
    let agent_name = agent.display_name().to_ascii_lowercase();
    let agent_id = agent.as_str().to_ascii_lowercase();
    lower == format!("{agent_name} · oauth")
        || lower == format!("{agent_name} ·oauth")
        || lower == format!("{agent_name} oauth")
        || lower == format!("{agent_id} oauth")
        || lower == format!("{agent_id}-oauth")
        || lower == format!("{agent_id} · oauth")
        || lower.ends_with(" · oauth")
        || lower.ends_with(" oauth")
}

/// 写入 extra.identityLabel（及 email）供 UI 分组；不参与去重。
pub(super) fn attach_identity_meta(
    adapter: &dyn AgentAdapter,
    kind: AccountKind,
    credentials: &Value,
    label: &str,
    mut extra: Value,
) -> Value {
    let id_label = adapter.identity_label(kind, credentials, Some(label));
    if let Some(obj) = extra.as_object_mut() {
        if let Some(ref lab) = id_label {
            obj.insert("identityLabel".into(), json!(lab));
            if lab.contains('@') {
                obj.entry("email".to_string()).or_insert_with(|| json!(lab));
            }
        }
        if kind == AccountKind::ApiKey {
            if let Some(tail) = api_key_tail(credentials) {
                obj.entry("secretTail".to_string()).or_insert_with(|| json!(tail));
            }
        }
    } else if let Some(lab) = id_label {
        let mut map = serde_json::Map::new();
        map.insert("identityLabel".into(), json!(lab));
        if lab.contains('@') {
            map.insert("email".into(), json!(lab));
        }
        if kind == AccountKind::ApiKey {
            if let Some(tail) = api_key_tail(credentials) {
                map.entry("secretTail".to_string()).or_insert_with(|| json!(tail));
            }
        }
        if let Value::Object(old) = extra {
            for (k, v) in old {
                map.entry(k).or_insert(v);
            }
        }
        extra = Value::Object(map);
    }
    extra
}

/// 同授权或同 OAuth 身份的多条历史冗余：优先 current → 更早 created_at → 更小 id。
pub(super) fn pick_primary_authorization_match(mut matches: Vec<Account>) -> Option<Account> {
    if matches.is_empty() {
        return None;
    }
    matches.sort_by(|a, b| {
        b.is_current
            .cmp(&a.is_current)
            .then_with(|| a.created_at.cmp(&b.created_at))
            .then_with(|| a.id.cmp(&b.id))
    });
    matches.into_iter().next()
}

pub(super) fn validate_label(value: &str, field: &str, max_chars: usize) -> Result<()> {
    if value.is_empty() {
        return Err(AppError::InvalidArg(format!("{field} must not be empty")));
    }
    if value != value.trim() {
        return Err(AppError::InvalidArg(format!(
            "{field} must not have surrounding whitespace"
        )));
    }
    if value.chars().count() > max_chars {
        return Err(AppError::InvalidArg(format!(
            "{field} exceeds maximum length of {max_chars} characters"
        )));
    }
    if value.chars().any(|c| c.is_control()) {
        return Err(AppError::InvalidArg(format!(
            "{field} must not contain control characters"
        )));
    }
    Ok(())
}

pub(super) fn agent_rank(id: AgentId) -> usize {
    AgentId::ALL
        .iter()
        .position(|a| *a == id)
        .unwrap_or(usize::MAX)
}

pub(super) fn sort_accounts(items: &mut [Account]) {
    items.sort_by(|a, b| {
        agent_rank(a.agent_id)
            .cmp(&agent_rank(b.agent_id))
            .then_with(|| a.label.cmp(&b.label))
            .then_with(|| a.id.cmp(&b.id))
    });
}
