//! Per-edge AccountPicker: ordered members, cursor, and isolation (RFC §3).
//!
//! Hung on [`super::host`] `EdgeState`. Does not read storage. Health is an
//! in-memory snapshot; isolation never reveals sibling accounts to callers.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use super::runtime::{ResolvedAuth, UpstreamAuthReload};

#[cfg(test)]
mod tests;

/// Snapshot health used by the picker. Maps from account-row `AuthHealth`
/// at start; the picker does not invent new product states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberHealth {
    /// Eligible to take a new request.
    Renewable,
    /// Isolated until restore. Skipped by pick/failover.
    NeedsLogin,
    /// Unknown / NeedsAttention: one try; persistent 401 isolates.
    TryOnce,
}

impl MemberHealth {
    pub fn is_eligible(self) -> bool {
        matches!(self, Self::Renewable | Self::TryOnce)
    }
}

/// One resolved pool member. `auth` is a shared cell so 401 reload can
/// replace the bearer in place without restarting the listener.
#[derive(Clone)]
pub struct PickedMember {
    pub ticket_id: String,
    pub source_kind: String,
    pub source_id: String,
    pub label: String,
    pub auth: ResolvedAuth,
    pub reload: Option<UpstreamAuthReload>,
    health: Arc<Mutex<MemberHealth>>,
}

impl std::fmt::Debug for PickedMember {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PickedMember")
            .field("ticket_id", &self.ticket_id)
            .field("source_kind", &self.source_kind)
            .field("source_id", &self.source_id)
            .field("label", &self.label)
            .field("auth", &self.auth)
            .field("reload", &self.reload.is_some())
            .field("health", &self.health())
            .finish()
    }
}

impl PickedMember {
    pub fn new(
        ticket_id: impl Into<String>,
        source_kind: impl Into<String>,
        source_id: impl Into<String>,
        label: impl Into<String>,
        auth: ResolvedAuth,
        reload: Option<UpstreamAuthReload>,
        health: MemberHealth,
    ) -> Self {
        Self {
            ticket_id: ticket_id.into(),
            source_kind: source_kind.into(),
            source_id: source_id.into(),
            label: label.into(),
            auth,
            reload,
            health: Arc::new(Mutex::new(health)),
        }
    }

    pub fn health(&self) -> MemberHealth {
        self.health
            .lock()
            .map(|guard| *guard)
            .unwrap_or(MemberHealth::NeedsLogin)
    }

    pub fn set_health(&self, health: MemberHealth) {
        if let Ok(mut guard) = self.health.lock() {
            *guard = health;
        }
    }

    pub fn is_eligible(&self) -> bool {
        self.health().is_eligible() && !self.auth.token().trim().is_empty()
    }

    /// Mark this member NeedsLogin. Does not touch other members' tokens.
    pub fn isolate(&self) {
        self.set_health(MemberHealth::NeedsLogin);
    }
}

/// Start-spec member list. Same shape as a live [`PickedMember`] without the
/// shared health cell until the picker materializes it.
#[derive(Clone)]
pub struct BridgeMemberSpec {
    pub ticket_id: String,
    pub source_kind: String,
    pub source_id: String,
    pub label: String,
    pub auth: ResolvedAuth,
    pub reload: Option<UpstreamAuthReload>,
    pub health: MemberHealth,
}

impl std::fmt::Debug for BridgeMemberSpec {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BridgeMemberSpec")
            .field("ticket_id", &self.ticket_id)
            .field("source_kind", &self.source_kind)
            .field("source_id", &self.source_id)
            .field("label", &self.label)
            .field("auth", &self.auth)
            .field("reload", &self.reload.is_some())
            .field("health", &self.health)
            .finish()
    }
}

impl From<&BridgeMemberSpec> for PickedMember {
    fn from(spec: &BridgeMemberSpec) -> Self {
        PickedMember::new(
            spec.ticket_id.clone(),
            spec.source_kind.clone(),
            spec.source_id.clone(),
            spec.label.clone(),
            spec.auth.clone(),
            spec.reload.clone(),
            spec.health,
        )
    }
}

/// Optional persist hook when a member is isolated. Host/controller may
/// write account-row health; the picker itself never opens storage.
pub type MemberHealthSink = Arc<dyn Fn(&str) + Send + Sync>;

/// Fixed-order poller hung on one edge. Cursor is process-local (restart → 0).
#[derive(Clone)]
pub struct AccountPicker {
    inner: Arc<AccountPickerInner>,
}

struct AccountPickerInner {
    members: Vec<PickedMember>,
    cursor: AtomicUsize,
    multi_account: bool,
    isolate_sink: Option<MemberHealthSink>,
}

impl AccountPicker {
    pub fn new(members: Vec<PickedMember>, multi_account: bool) -> Self {
        Self::with_sink(members, multi_account, None)
    }

    pub fn with_sink(
        members: Vec<PickedMember>,
        multi_account: bool,
        isolate_sink: Option<MemberHealthSink>,
    ) -> Self {
        Self {
            inner: Arc::new(AccountPickerInner {
                members,
                cursor: AtomicUsize::new(0),
                multi_account,
                isolate_sink,
            }),
        }
    }

    /// RFC §7: even if the spec listed siblings, a closed gate keeps only lead.
    pub fn from_members(
        members: Vec<PickedMember>,
        multi_account: bool,
        isolate_sink: Option<MemberHealthSink>,
    ) -> Self {
        let members = if multi_account || members.len() <= 1 {
            members
        } else {
            members.into_iter().take(1).collect()
        };
        Self::with_sink(members, multi_account, isolate_sink)
    }

    pub fn multi_account(&self) -> bool {
        self.inner.multi_account
    }

    pub fn members(&self) -> &[PickedMember] {
        &self.inner.members
    }

    pub fn len(&self) -> usize {
        self.inner.members.len()
    }

    /// Mix Grok session/replay by account only when polling is actually on.
    pub fn partition_account_id<'a>(&self, member: &'a PickedMember) -> Option<&'a str> {
        if self.inner.multi_account && self.inner.members.len() > 1 {
            Some(member.source_id.as_str())
        } else {
            None
        }
    }

    /// New request: from cursor, first eligible member; then cursor = (idx+1)%n.
    pub fn pick_new(&self) -> Option<PickedMember> {
        let n = self.inner.members.len();
        if n == 0 {
            return None;
        }
        let start = self.inner.cursor.load(Ordering::SeqCst) % n;
        for offset in 0..n {
            let idx = (start + offset) % n;
            let member = &self.inner.members[idx];
            if member.is_eligible() {
                self.inner.cursor.store((idx + 1) % n, Ordering::SeqCst);
                return Some(member.clone());
            }
        }
        None
    }

    /// Same-request failover: next eligible after `from`, cursor unchanged.
    pub fn failover(&self, from_source_id: &str) -> Option<PickedMember> {
        let n = self.inner.members.len();
        if n == 0 {
            return None;
        }
        let start = self
            .inner
            .members
            .iter()
            .position(|member| member.source_id == from_source_id)
            .map(|idx| idx + 1)
            .unwrap_or(0);
        for offset in 0..n {
            let idx = (start + offset) % n;
            let member = &self.inner.members[idx];
            if member.source_id == from_source_id {
                continue;
            }
            if member.is_eligible() {
                return Some(member.clone());
            }
        }
        None
    }

    pub fn isolate(&self, source_id: &str) {
        if let Some(member) = self
            .inner
            .members
            .iter()
            .find(|member| member.source_id == source_id)
        {
            member.isolate();
        }
        if let Some(sink) = &self.inner.isolate_sink {
            sink(source_id);
        }
    }

    pub fn restore(&self, source_id: &str, health: MemberHealth) {
        if let Some(member) = self
            .inner
            .members
            .iter()
            .find(|member| member.source_id == source_id)
        {
            member.set_health(health);
        }
    }

    pub fn health_of(&self, source_id: &str) -> Option<MemberHealth> {
        self.inner
            .members
            .iter()
            .find(|member| member.source_id == source_id)
            .map(PickedMember::health)
    }
}
