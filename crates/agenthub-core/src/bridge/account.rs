//! Per-edge AccountPicker: ordered members, cursor, and isolation (RFC §3).
//!
//! Hung on [`super::host`] `EdgeState`. Does not read storage. Health is an
//! in-memory snapshot; isolation never reveals sibling accounts to callers.
//! Cooldown is process-local and is not a capability change.

use std::cmp::Ordering as CmpOrdering;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::http::HeaderValue;
use sha2::{Digest, Sha256};

use super::route_index::DispatchCandidate;
use super::runtime::{ResolvedAuth, UpstreamAuthReload};
use crate::models::RouteSchedulePolicy;

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

/// Per-pool sticky record. Keyed by [`route_scoped_affinity_key`], never by
/// the raw session string, and never shared across edges/bearers.
#[derive(Clone, Debug)]
struct StickyBinding {
    member_id: String,
    provider: String,
    upstream_dialect: String,
    index_generation: u64,
    auth_fingerprint: String,
}

struct EligibleMember<'a> {
    member: &'a PickedMember,
    candidate: &'a DispatchCandidate,
}

enum StickyLookup {
    Hit(PickedMember),
    /// Bound member is still valid in the resolver set but skipped for this
    /// request (cooldown, isolation wait, or this-request exclusion).
    Held,
    Miss,
}

struct AccountPickerInner {
    members: Vec<PickedMember>,
    cursor: AtomicUsize,
    multi_account: bool,
    isolate_sink: Option<MemberHealthSink>,
    cooldowns: Mutex<MemberCooldowns>,
    schedule_policy: RouteSchedulePolicy,
    sticky: Mutex<HashMap<String, StickyBinding>>,
    /// Round-robin cursors keyed by isomorphic group (priority + transport + dialect).
    /// Distinct from the v1 [`AccountPicker::pick_new`] cursor.
    rr_cursors: Mutex<HashMap<String, usize>>,
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
        Self::with_policy(
            members,
            multi_account,
            isolate_sink,
            RouteSchedulePolicy::PriorityFailover,
        )
    }

    pub fn with_policy(
        members: Vec<PickedMember>,
        multi_account: bool,
        isolate_sink: Option<MemberHealthSink>,
        schedule_policy: RouteSchedulePolicy,
    ) -> Self {
        Self {
            inner: Arc::new(AccountPickerInner {
                members,
                cursor: AtomicUsize::new(0),
                multi_account,
                isolate_sink,
                schedule_policy,
                cooldowns: Mutex::new(MemberCooldowns {
                    member: HashMap::new(),
                    member_model: HashMap::new(),
                }),
                sticky: Mutex::new(HashMap::new()),
                rr_cursors: Mutex::new(HashMap::new()),
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

    /// Shrink `resolve` output only. Sticky (if valid) beats policy; otherwise
    /// `priority_failover` (stable) or pool-scoped `round_robin` among the
    /// highest-priority isomorphic group.
    pub fn pick_from_candidates(
        &self,
        candidates: &[DispatchCandidate],
        affinity_key: Option<&str>,
        excluded_members: &[String],
    ) -> Option<PickedMember> {
        let eligible: Vec<EligibleMember<'_>> = self
            .inner
            .members
            .iter()
            .filter_map(|member| {
                if !member.is_eligible() || Self::is_excluded(member, excluded_members) {
                    return None;
                }
                let candidate = candidates
                    .iter()
                    .find(|candidate| Self::matches_candidate(member, candidate))?;
                Some(EligibleMember { member, candidate })
            })
            .collect();
        if eligible.is_empty() {
            return None;
        }
        let affinity = affinity_key
            .map(str::trim)
            .filter(|key| !key.is_empty())
            .map(str::to_owned);
        let mut sticky_held = false;
        if let Some(key) = affinity.as_deref() {
            match self.lookup_sticky(candidates, excluded_members, key) {
                StickyLookup::Hit(picked) => return Some(picked),
                StickyLookup::Held => sticky_held = true,
                StickyLookup::Miss => {}
            }
        }
        let picked = match self.inner.schedule_policy {
            RouteSchedulePolicy::RoundRobin => self.pick_round_robin(&eligible),
            RouteSchedulePolicy::PriorityFailover => pick_priority_failover(&eligible),
        }?;
        if let Some(key) = affinity.as_deref() {
            if !sticky_held {
                if let Some(candidate) = matching_candidate(&picked, candidates) {
                    self.record_sticky(key, &picked, candidate);
                }
            }
        }
        Some(picked)
    }

    /// Prefer a still-valid sticky member from the full resolver set, before
    /// lane filtering. Cooldown / this-request exclusion skips the member for
    /// this pick without deleting the binding.
    pub fn try_sticky(
        &self,
        candidates: &[DispatchCandidate],
        affinity_key: Option<&str>,
        excluded_members: &[String],
    ) -> Option<PickedMember> {
        let key = affinity_key
            .map(str::trim)
            .filter(|value| !value.is_empty())?;
        match self.lookup_sticky(candidates, excluded_members, key) {
            StickyLookup::Hit(picked) => Some(picked),
            StickyLookup::Held | StickyLookup::Miss => None,
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
        self.drop_sticky_for_member(source_id);
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

    fn lookup_sticky(
        &self,
        universe: &[DispatchCandidate],
        excluded_members: &[String],
        key: &str,
    ) -> StickyLookup {
        let binding = {
            let Ok(guard) = self.inner.sticky.lock() else {
                return StickyLookup::Miss;
            };
            guard.get(key).cloned()
        };
        let Some(binding) = binding else {
            return StickyLookup::Miss;
        };
        let Some(member) = self
            .inner
            .members
            .iter()
            .find(|member| sticky_member_matches(member, &binding.member_id))
        else {
            self.drop_sticky(key);
            return StickyLookup::Miss;
        };
        let Some(candidate) = universe
            .iter()
            .find(|candidate| Self::matches_candidate(member, candidate))
        else {
            self.drop_sticky(key);
            return StickyLookup::Miss;
        };
        if !sticky_still_valid(&binding, member, candidate) || !member.is_eligible() {
            self.drop_sticky(key);
            return StickyLookup::Miss;
        }
        if Self::is_excluded(member, excluded_members) {
            return StickyLookup::Held;
        }
        StickyLookup::Hit(member.clone())
    }

    fn pick_round_robin(&self, eligible: &[EligibleMember<'_>]) -> Option<PickedMember> {
        let mut ordered: Vec<&EligibleMember<'_>> = eligible.iter().collect();
        ordered.sort_by(|left, right| cmp_schedule(left, right));
        let lead = *ordered.first()?;
        let group: Vec<&EligibleMember<'_>> = ordered
            .iter()
            .copied()
            .filter(|item| {
                item.member.priority == lead.member.priority
                    && item.candidate.transport_key == lead.candidate.transport_key
                    && item.candidate.upstream_dialect == lead.candidate.upstream_dialect
            })
            .collect();
        let n = group.len();
        if n == 0 {
            return None;
        }
        let cursor_key = isomorphic_cursor_key(
            lead.member.priority,
            &lead.candidate.transport_key,
            &lead.candidate.upstream_dialect,
        );
        let start = match self.inner.rr_cursors.lock() {
            Ok(mut cursors) => {
                let slot = cursors.entry(cursor_key).or_insert(0);
                let start = *slot % n;
                *slot = (start + 1) % n;
                start
            }
            Err(_) => 0,
        };
        Some(group[start].member.clone())
    }

    fn record_sticky(&self, key: &str, member: &PickedMember, candidate: &DispatchCandidate) {
        let Ok(mut guard) = self.inner.sticky.lock() else {
            return;
        };
        guard.insert(
            key.to_owned(),
            StickyBinding {
                member_id: member.source_id.clone(),
                provider: candidate.upstream_provider.clone(),
                upstream_dialect: candidate.upstream_dialect.clone(),
                index_generation: candidate.capability_generation,
                auth_fingerprint: member.authorization_fingerprint(),
            },
        );
    }

    fn drop_sticky(&self, key: &str) {
        if let Ok(mut guard) = self.inner.sticky.lock() {
            guard.remove(key);
        }
    }

    fn drop_sticky_for_member(&self, source_id: &str) {
        let Ok(mut guard) = self.inner.sticky.lock() else {
            return;
        };
        guard.retain(|_, binding| {
            binding.member_id != source_id
                && !self.inner.members.iter().any(|member| {
                    member.source_id == source_id
                        && sticky_member_matches(member, &binding.member_id)
                })
        });
    }

    #[cfg(test)]
    pub(super) fn rewrite_sticky_fingerprint(&self, affinity_key: &str, fingerprint: &str) {
        let Ok(mut guard) = self.inner.sticky.lock() else {
            return;
        };
        if let Some(binding) = guard.get_mut(affinity_key) {
            binding.auth_fingerprint = fingerprint.to_owned();
        }
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

/// Route-scoped sticky key: `(route_id, downstream_dialect, hash(session))`.
/// Never the raw session string, and never a process-global map key.
pub fn route_scoped_affinity_key(
    route_id: &str,
    downstream_dialect: &str,
    session_identifier: &str,
) -> String {
    format!(
        "{}\x1f{}\x1f{}",
        route_id.trim(),
        downstream_dialect.trim(),
        hash_session_identifier(session_identifier.trim())
    )
}

fn hash_session_identifier(session: &str) -> String {
    let digest = Sha256::digest(session.as_bytes());
    digest
        .iter()
        .take(16)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn matching_candidate<'a>(
    member: &PickedMember,
    candidates: &'a [DispatchCandidate],
) -> Option<&'a DispatchCandidate> {
    candidates
        .iter()
        .find(|candidate| AccountPicker::matches_candidate(member, candidate))
}

fn sticky_member_matches(member: &PickedMember, bound_id: &str) -> bool {
    member.source_id == bound_id || member.ticket_id == bound_id || member.label == bound_id
}

fn sticky_still_valid(
    binding: &StickyBinding,
    member: &PickedMember,
    candidate: &DispatchCandidate,
) -> bool {
    // Index generation is stored on the binding so a later snapshot can
    // re-validate. A bump that still includes this member with the same
    // provider/dialect stays valid; a bump that dropped the member never
    // reaches here because `eligible` is the current resolver set.
    let _ = binding.index_generation;
    member.is_eligible()
        && binding.provider == candidate.upstream_provider
        && binding.upstream_dialect == candidate.upstream_dialect
        && binding.auth_fingerprint == member.authorization_fingerprint()
}

fn cmp_schedule(left: &EligibleMember<'_>, right: &EligibleMember<'_>) -> CmpOrdering {
    left.member
        .priority
        .cmp(&right.member.priority)
        .then(left.member.position.cmp(&right.member.position))
        .then(left.member.source_id.cmp(&right.member.source_id))
}

fn pick_priority_failover(eligible: &[EligibleMember<'_>]) -> Option<PickedMember> {
    let mut ordered: Vec<&EligibleMember<'_>> = eligible.iter().collect();
    ordered.sort_by(|left, right| cmp_schedule(left, right));
    ordered.first().map(|item| item.member.clone())
}

fn isomorphic_cursor_key(priority: i64, transport_key: &str, dialect: &str) -> String {
    format!("{priority}\x1f{transport_key}\x1f{dialect}")
}
