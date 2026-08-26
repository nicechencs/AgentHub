//! Per-edge AccountPicker: ordered members, cursor, and isolation (RFC §3).
//!
//! Hung on [`super::host`] `EdgeState`. Does not read storage. Health is an
//! in-memory snapshot; isolation never reveals sibling accounts to callers.
//! Cooldown is process-local and is not a capability change.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::http::HeaderValue;

use super::route_index::DispatchCandidate;
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
    pub priority: i64,
    pub position: i64,
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
            .field("priority", &self.priority)
            .field("position", &self.position)
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
            priority: 0,
            position: 0,
            health: Arc::new(Mutex::new(health)),
        }
    }

    pub fn with_schedule(mut self, priority: i64, position: i64) -> Self {
        self.priority = priority;
        self.position = position;
        self
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

    /// Authorization identity shared across RoutePools for reload/isolate.
    pub fn authorization_fingerprint(&self) -> String {
        if !self.ticket_id.trim().is_empty() {
            self.ticket_id.clone()
        } else if !self.source_id.trim().is_empty() {
            format!("{}:{}", self.source_kind, self.source_id)
        } else {
            self.source_id.clone()
        }
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
    pub priority: i64,
    pub position: i64,
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
            .field("priority", &self.priority)
            .field("position", &self.position)
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
        .with_schedule(spec.priority, spec.position)
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

struct MemberCooldowns {
    member: HashMap<String, Instant>,
    member_model: HashMap<(String, String), Instant>,
}

struct AccountPickerInner {
    members: Vec<PickedMember>,
    cursor: AtomicUsize,
    multi_account: bool,
    isolate_sink: Option<MemberHealthSink>,
    cooldowns: Mutex<MemberCooldowns>,
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
                cooldowns: Mutex::new(MemberCooldowns {
                    member: HashMap::new(),
                    member_model: HashMap::new(),
                }),
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

    fn matches_candidate(member: &PickedMember, candidate: &DispatchCandidate) -> bool {
        member.source_id == candidate.member_id
            || member.ticket_id == candidate.member_id
            || member.label == candidate.member_id
    }

    fn is_excluded(member: &PickedMember, excluded_members: &[String]) -> bool {
        excluded_members.iter().any(|excluded| {
            excluded == &member.source_id
                || excluded == &member.ticket_id
                || excluded == &member.label
        })
    }

    /// Shrink `resolve` output only. Order is priority, position, id.
    pub fn pick_from_candidates(
        &self,
        candidates: &[DispatchCandidate],
        affinity_key: Option<&str>,
        excluded_members: &[String],
    ) -> Option<PickedMember> {
        let mut eligible: Vec<&PickedMember> = self
            .inner
            .members
            .iter()
            .filter(|member| {
                candidates
                    .iter()
                    .any(|candidate| Self::matches_candidate(member, candidate))
                    && member.is_eligible()
                    && !Self::is_excluded(member, excluded_members)
            })
            .collect();
        if eligible.is_empty() {
            return None;
        }
        if let Some(affinity) = affinity_key.map(str::trim).filter(|key| !key.is_empty()) {
            if let Some(sticky) = eligible.iter().find(|member| {
                member.source_id == affinity
                    || member.ticket_id == affinity
                    || member.label == affinity
            }) {
                return Some((*sticky).clone());
            }
        }
        eligible.sort_by(|left, right| {
            left.priority
                .cmp(&right.priority)
                .then(left.position.cmp(&right.position))
                .then(left.source_id.cmp(&right.source_id))
        });
        eligible.first().map(|member| (*member).clone())
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

    /// `model = None` cools the whole member; otherwise only that model bucket.
    /// Concurrent 429s keep the later deadline so a short `Retry-After` cannot
    /// replace a longer one.
    pub fn set_cooldown(&self, member_id: &str, model: Option<&str>, duration: Duration) {
        let until = Instant::now() + duration;
        let Ok(mut guard) = self.inner.cooldowns.lock() else {
            return;
        };
        match model.map(str::trim).filter(|value| !value.is_empty()) {
            Some(model) => keep_later_deadline(
                guard
                    .member_model
                    .entry((member_id.to_owned(), model.to_owned()))
                    .or_insert(until),
                until,
            ),
            None => keep_later_deadline(
                guard.member.entry(member_id.to_owned()).or_insert(until),
                until,
            ),
        }
    }

    pub fn is_cooling(&self, member_id: &str, model: Option<&str>) -> bool {
        let now = Instant::now();
        let Ok(guard) = self.inner.cooldowns.lock() else {
            return false;
        };
        if guard
            .member
            .get(member_id)
            .is_some_and(|until| now < *until)
        {
            return true;
        }
        let Some(model) = model.map(str::trim).filter(|value| !value.is_empty()) else {
            return false;
        };
        guard
            .member_model
            .get(&(member_id.to_owned(), model.to_owned()))
            .is_some_and(|until| now < *until)
    }

    /// Members that must not be picked for `model` right now.
    pub fn cooldown_exclusions(&self, model: &str) -> Vec<String> {
        self.inner
            .members
            .iter()
            .filter(|member| self.is_cooling(&member.source_id, Some(model)))
            .map(|member| member.source_id.clone())
            .collect()
    }

    pub fn soonest_retry_after(&self, model: &str) -> Option<HeaderValue> {
        let now = Instant::now();
        let Ok(guard) = self.inner.cooldowns.lock() else {
            return None;
        };
        let mut remaining: Option<Duration> = None;
        let take_until = |until: Instant, remaining: &mut Option<Duration>| {
            if now < until {
                let wait = until.saturating_duration_since(now);
                *remaining = Some(remaining.map_or(wait, |current| current.min(wait)));
            }
        };
        for member in &self.inner.members {
            if let Some(until) = guard.member.get(&member.source_id) {
                take_until(*until, &mut remaining);
            }
            if let Some(until) = guard
                .member_model
                .get(&(member.source_id.clone(), model.to_owned()))
            {
                take_until(*until, &mut remaining);
            }
        }
        remaining.map(|wait| {
            let secs = wait.as_secs().max(1);
            HeaderValue::from_str(&secs.to_string())
                .unwrap_or_else(|_| HeaderValue::from_static("1"))
        })
    }
}

fn keep_later_deadline(existing: &mut Instant, until: Instant) {
    if until > *existing {
        *existing = until;
    }
}
