//! Process-local 401 reload singleflight, keyed by authorization fingerprint.
//!
//! Shared across RoutePools on the same host so concurrent 401s refresh once.
//! Model catalogs and affinity stay per pool.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use super::account::PickedMember;

#[cfg(test)]
mod tests;

/// Outcome of one coordinated reload. Waiters re-read after the in-flight call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthReloadOutcome {
    /// This cell now holds a new token.
    Rotated,
    /// Another waiter already applied a new token; retry with the current cell.
    AlreadyFresh,
    /// Reload ran (or there was no callback) and the token did not change.
    Unchanged,
    /// A newer credential landed while reload ran; do not clobber it, retry.
    Stale,
}

impl AuthReloadOutcome {
    pub fn should_retry(self) -> bool {
        matches!(self, Self::Rotated | Self::AlreadyFresh | Self::Stale)
    }
}

#[derive(Clone)]
struct SharedReload {
    outcome: AuthReloadOutcome,
    token: Option<String>,
}

struct FingerprintGate {
    lock: tokio::sync::Mutex<()>,
    generation: AtomicU64,
    shared: Mutex<Option<SharedReload>>,
}

impl FingerprintGate {
    fn new() -> Self {
        Self {
            lock: tokio::sync::Mutex::new(()),
            generation: AtomicU64::new(0),
            shared: Mutex::new(None),
        }
    }
}

struct AuthReloadInner {
    gates: Mutex<HashMap<String, Arc<FingerprintGate>>>,
    isolated: Mutex<HashSet<String>>,
}

/// Host-scoped coordinator. Isolate/reload are shared; catalogs are not.
#[derive(Clone)]
pub struct AuthReloadCoordinator {
    inner: Arc<AuthReloadInner>,
}

impl AuthReloadCoordinator {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(AuthReloadInner {
                gates: Mutex::new(HashMap::new()),
                isolated: Mutex::new(HashSet::new()),
            }),
        }
    }

    fn gate(&self, fingerprint: &str) -> Arc<FingerprintGate> {
        let mut gates = self
            .inner
            .gates
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        gates
            .entry(fingerprint.to_owned())
            .or_insert_with(|| Arc::new(FingerprintGate::new()))
            .clone()
    }

    pub fn is_isolated(&self, fingerprint: &str) -> bool {
        self.inner
            .isolated
            .lock()
            .map(|set| set.contains(fingerprint))
            .unwrap_or(true)
    }

    pub fn isolate(&self, fingerprint: &str) {
        if fingerprint.trim().is_empty() {
            return;
        }
        if let Ok(mut set) = self.inner.isolated.lock() {
            set.insert(fingerprint.to_owned());
        }
    }

    pub fn clear_isolated(&self, fingerprint: &str) {
        if let Ok(mut set) = self.inner.isolated.lock() {
            set.remove(fingerprint);
        }
    }

    /// One refresh per fingerprint. Waiters reuse the leader outcome; they
    /// never apply a previous rotation over a newer revision.
    pub async fn reload_member(&self, member: &PickedMember) -> AuthReloadOutcome {
        let fingerprint = member.authorization_fingerprint();
        if fingerprint.trim().is_empty() {
            return AuthReloadOutcome::Unchanged;
        }
        let gate = self.gate(&fingerprint);
        let gen_before = gate.generation.load(Ordering::SeqCst);
        let observed = member.auth.revision();
        let _lock = gate.lock.lock().await;
        let gen_after = gate.generation.load(Ordering::SeqCst);
        if gen_after > gen_before {
            return apply_shared_reload(member, &gate, observed);
        }
        let outcome = run_reload(member);
        let token = matches!(
            outcome,
            AuthReloadOutcome::Rotated | AuthReloadOutcome::Stale
        )
        .then(|| member.auth.token());
        {
            let mut shared = gate
                .shared
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            *shared = Some(SharedReload { outcome, token });
        }
        gate.generation.fetch_add(1, Ordering::SeqCst);
        outcome
    }
}

impl Default for AuthReloadCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

fn apply_shared_reload(
    member: &PickedMember,
    gate: &FingerprintGate,
    observed: u64,
) -> AuthReloadOutcome {
    let shared = gate
        .shared
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    let Some(shared) = shared else {
        return AuthReloadOutcome::Unchanged;
    };
    match shared.outcome {
        AuthReloadOutcome::Unchanged => AuthReloadOutcome::Unchanged,
        AuthReloadOutcome::Rotated => {
            if let Some(token) = shared.token.as_deref() {
                member.auth.apply_reloaded_token(observed, token);
            }
            AuthReloadOutcome::AlreadyFresh
        }
        AuthReloadOutcome::Stale => {
            if let Some(token) = shared.token.as_deref() {
                member.auth.apply_reloaded_token(observed, token);
            }
            AuthReloadOutcome::Stale
        }
        AuthReloadOutcome::AlreadyFresh => AuthReloadOutcome::AlreadyFresh,
    }
}

fn run_reload(member: &PickedMember) -> AuthReloadOutcome {
    let Some(reload) = member.reload.as_ref() else {
        return AuthReloadOutcome::Unchanged;
    };
    let observed = member.auth.revision();
    let current = member.auth.token();
    let Some(next) = reload() else {
        return AuthReloadOutcome::Unchanged;
    };
    let next = next.trim();
    if next.is_empty() {
        return AuthReloadOutcome::Unchanged;
    }
    if next == current {
        // Cell already holds this token (prior rotation won, or refresh was a
        // no-op). Retry with it instead of isolating a still-usable member.
        return AuthReloadOutcome::AlreadyFresh;
    }
    if member.auth.replace_token_at_revision(observed, next) {
        AuthReloadOutcome::Rotated
    } else {
        AuthReloadOutcome::Stale
    }
}
