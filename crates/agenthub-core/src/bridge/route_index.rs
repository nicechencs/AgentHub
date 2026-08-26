//! Shared resolver snapshot for v2 route pools.
//!
//! `resolve` and `list_models` read the same map. Absent index keeps v1 lead
//! + `switch_edge_for_model`; a present index fail-closes unknown and
//! ambiguous models and does not scan sibling profiles.

use std::collections::{BTreeMap, BTreeSet};

use crate::models::ModelRouteRule;

#[cfg(test)]
mod tests;

/// Stable capability of one member for one public model / endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberCapability {
    Supported,
    Unsupported,
    Unknown,
}

/// One member's stable grant used to build [`EffectiveRouteIndex`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberCapabilitySnapshot {
    pub member_id: String,
    pub public_model: String,
    pub endpoint: String,
    pub upstream_provider: String,
    pub upstream_dialect: String,
    pub upstream_model: String,
    pub upstream_endpoint: String,
    pub transport_key: String,
    pub capability: MemberCapability,
}

/// One eligible upstream attempt after model / endpoint / dialect resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatchCandidate {
    pub member_id: String,
    pub upstream_endpoint: String,
    pub upstream_model: String,
    pub upstream_provider: String,
    pub upstream_dialect: String,
    pub transport_key: String,
    pub capability_generation: u64,
}

/// Fail-closed outcomes from [`EffectiveRouteIndex::resolve`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteResolveError {
    UnknownModel,
    AmbiguousModel,
    EmptyIndex,
}

/// Why a public model or lane was refused. Tests distinguish these; no UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteRejectionReason {
    AmbiguousNoRule,
    NoMatchingRule,
    UnknownModel,
    LaneDisabled,
}

/// `(route, endpoint, public_model) → DispatchCandidate[]` built from member
/// snapshots. `/models` enumerates the same keys. Mixed-provider candidates
/// stay fail-closed until `feature.mixed_provider_pool` plus explicit rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveRouteIndex {
    pub route_id: String,
    pub generation: u64,
    by_endpoint_model: BTreeMap<(String, String), Vec<DispatchCandidate>>,
    mixed_provider_enabled: bool,
    rules: Vec<ModelRouteRule>,
}

impl EffectiveRouteIndex {
    pub fn build(
        route_id: impl Into<String>,
        generation: u64,
        snapshots: &[MemberCapabilitySnapshot],
    ) -> Self {
        let mut by_endpoint_model: BTreeMap<(String, String), Vec<DispatchCandidate>> =
            BTreeMap::new();
        for snapshot in snapshots {
            if snapshot.capability != MemberCapability::Supported {
                continue;
            }
            let public_model = snapshot.public_model.trim();
            let endpoint = snapshot.endpoint.trim();
            if public_model.is_empty()
                || endpoint.is_empty()
                || snapshot.member_id.trim().is_empty()
            {
                continue;
            }
            let candidate = DispatchCandidate {
                member_id: snapshot.member_id.clone(),
                upstream_endpoint: snapshot.upstream_endpoint.clone(),
                upstream_model: if snapshot.upstream_model.trim().is_empty() {
                    public_model.to_owned()
                } else {
                    snapshot.upstream_model.clone()
                },
                upstream_provider: snapshot.upstream_provider.clone(),
                upstream_dialect: snapshot.upstream_dialect.clone(),
                transport_key: snapshot.transport_key.clone(),
                capability_generation: generation,
            };
            by_endpoint_model
                .entry((endpoint.to_owned(), public_model.to_owned()))
                .or_default()
                .push(candidate);
        }
        for candidates in by_endpoint_model.values_mut() {
            candidates.sort_by(|left, right| left.member_id.cmp(&right.member_id));
            candidates.dedup_by(|left, right| left.member_id == right.member_id);
        }
        Self {
            route_id: route_id.into(),
            generation,
            by_endpoint_model,
            mixed_provider_enabled: false,
            rules: Vec::new(),
        }
    }

    /// Attach operator-authored rules. Flag off ignores rules and keeps
    /// mixed-provider indexes fail-closed (`AmbiguousModel`).
    pub fn with_mixed_provider_rules(mut self, enabled: bool, rules: Vec<ModelRouteRule>) -> Self {
        self.mixed_provider_enabled = enabled;
        self.rules = rules;
        if enabled {
            self.project_public_models_from_rules();
        }
        self
    }

    pub fn mixed_provider_enabled(&self) -> bool {
        self.mixed_provider_enabled
    }

    pub fn rules(&self) -> &[ModelRouteRule] {
        &self.rules
    }

    pub fn resolve(
        &self,
        endpoint: &str,
        public_model: &str,
    ) -> Result<Vec<DispatchCandidate>, RouteResolveError> {
        if self.by_endpoint_model.is_empty() {
            return Err(RouteResolveError::EmptyIndex);
        }
        let model = public_model.trim();
        if model.is_empty() {
            return Err(RouteResolveError::UnknownModel);
        }
        let Some(candidates) = self
            .by_endpoint_model
            .get(&(endpoint.trim().to_owned(), model.to_owned()))
        else {
            return Err(RouteResolveError::UnknownModel);
        };
        if candidates.is_empty() {
            return Err(RouteResolveError::UnknownModel);
        }
        let mixed = distinct_providers(candidates).len() > 1;
        let enabled_rules = self.enabled_rules_for(endpoint, model);
        if mixed && (!self.mixed_provider_enabled || enabled_rules.is_empty()) {
            return Err(RouteResolveError::AmbiguousModel);
        }
        if !self.mixed_provider_enabled || enabled_rules.is_empty() {
            return Ok(candidates.clone());
        }
        let filtered = filter_candidates_by_rules(candidates, &enabled_rules);
        if filtered.is_empty() {
            return Err(RouteResolveError::UnknownModel);
        }
        Ok(filtered)
    }

    /// Public model ids that currently have at least one supported candidate.
    /// Same predicate as `resolve`: a model is listed iff resolve succeeds.
    pub fn list_models(&self, endpoint: &str) -> Vec<String> {
        let endpoint = endpoint.trim();
        let mut models = Vec::new();
        for (snap_endpoint, model) in self.by_endpoint_model.keys() {
            if snap_endpoint != endpoint {
                continue;
            }
            if self.resolve(endpoint, model).is_ok() {
                models.push(model.clone());
            }
        }
        models
    }

    /// Model-level refusal. `Ok` resolve yields `None`.
    pub fn explain(&self, endpoint: &str, public_model: &str) -> Option<RouteRejectionReason> {
        match self.resolve(endpoint, public_model) {
            Ok(_) => None,
            Err(RouteResolveError::AmbiguousModel) => {
                Some(self.explain_ambiguous(endpoint, public_model))
            }
            Err(RouteResolveError::UnknownModel) | Err(RouteResolveError::EmptyIndex) => {
                Some(self.explain_unknown(endpoint, public_model))
            }
        }
    }

    /// Lane-level refusal for tests / management state. `None` if the lane
    /// would be eligible for this public model.
    pub fn explain_lane(
        &self,
        endpoint: &str,
        public_model: &str,
        upstream_provider: &str,
        upstream_dialect: &str,
    ) -> Option<RouteRejectionReason> {
        let endpoint = endpoint.trim();
        let model = public_model.trim();
        let provider = upstream_provider.trim();
        let dialect = upstream_dialect.trim();
        let lane_rules: Vec<&ModelRouteRule> = self
            .rules
            .iter()
            .filter(|rule| {
                rule.public_model.trim() == model
                    && rule.endpoint_family.trim() == endpoint
                    && rule.upstream_provider.trim() == provider
                    && rule.upstream_dialect.trim() == dialect
            })
            .collect();
        let enabled: Vec<&ModelRouteRule> = lane_rules
            .iter()
            .copied()
            .filter(|rule| rule.enabled)
            .collect();
        let disabled_only = enabled.is_empty() && lane_rules.iter().any(|rule| !rule.enabled);
        if disabled_only {
            return Some(RouteRejectionReason::LaneDisabled);
        }
        let snapshots = self.snapshots_for(endpoint, model);
        if snapshots.is_empty() {
            return Some(RouteRejectionReason::UnknownModel);
        }
        if enabled.is_empty() {
            if distinct_providers(&candidates_from_snapshots(&snapshots)).len() > 1 {
                return Some(RouteRejectionReason::AmbiguousNoRule);
            }
            return None;
        }
        let matched = snapshots.iter().any(|snapshot| {
            enabled
                .iter()
                .any(|rule| rule_matches_snapshot(rule, snapshot))
        });
        if matched {
            None
        } else {
            Some(RouteRejectionReason::NoMatchingRule)
        }
    }

    /// Restrict resolved candidates to the next lane the scheduler may try.
    ///
    /// First pick: lowest rule priority among lanes that still have a remaining
    /// candidate. Failover stays in that lane unless matching rules share a
    /// non-empty `equivalent_group`. Single-provider / flag-off indexes return
    /// the input set unchanged.
    pub fn schedule_lane(
        &self,
        endpoint: &str,
        public_model: &str,
        candidates: &[DispatchCandidate],
        excluded_member_ids: &[String],
        last_member_id: Option<&str>,
    ) -> Vec<DispatchCandidate> {
        if candidates.is_empty() {
            return Vec::new();
        }
        if !self.mixed_provider_enabled {
            return candidates.to_vec();
        }
        let remaining: Vec<&DispatchCandidate> = candidates
            .iter()
            .filter(|candidate| !member_excluded(candidate, excluded_member_ids))
            .collect();
        if remaining.is_empty() {
            return Vec::new();
        }
        let last_lane = last_member_id.and_then(|member_id| {
            candidates
                .iter()
                .find(|candidate| candidate_matches_id(candidate, member_id))
                .map(lane_key)
        });
        let mut lanes: BTreeSet<(&str, &str)> = BTreeSet::new();
        for candidate in &remaining {
            lanes.insert(lane_key(candidate));
        }
        // First pick with one remaining lane has nowhere else to go. After a
        // failed attempt, still run choose_lane so a non-equivalent hop is
        // refused even when only the other Provider is left.
        if last_lane.is_none() && lanes.len() <= 1 {
            return remaining.into_iter().cloned().collect();
        }
        let enabled_rules = self.enabled_rules_for(endpoint, public_model);
        let chosen = choose_lane(&remaining, &enabled_rules, last_lane);
        remaining
            .into_iter()
            .filter(|candidate| {
                candidate.upstream_provider == chosen.0 && candidate.upstream_dialect == chosen.1
            })
            .cloned()
            .collect()
    }

    fn project_public_models_from_rules(&mut self) {
        let snapshots = self.capability_snapshots();
        let generation = self.generation;
        let enabled: Vec<ModelRouteRule> = self
            .rules
            .iter()
            .filter(|rule| rule.enabled)
            .cloned()
            .collect();
        for rule in &enabled {
            let endpoint = rule.endpoint_family.trim();
            let public_model = rule.public_model.trim();
            let target_model = rule.upstream_model.trim();
            if endpoint.is_empty() || public_model.is_empty() {
                continue;
            }
            for snapshot in &snapshots {
                if snapshot.endpoint.trim() != endpoint {
                    continue;
                }
                if snapshot.upstream_provider.trim() != rule.upstream_provider.trim()
                    || snapshot.upstream_dialect.trim() != rule.upstream_dialect.trim()
                {
                    continue;
                }
                let listed = snapshot.public_model.trim();
                let upstream = snapshot.upstream_model.trim();
                if upstream != target_model && listed != target_model && listed != public_model {
                    continue;
                }
                let candidate = DispatchCandidate {
                    member_id: snapshot.member_id.clone(),
                    upstream_endpoint: snapshot.upstream_endpoint.clone(),
                    upstream_model: if target_model.is_empty() {
                        snapshot.upstream_model.clone()
                    } else {
                        target_model.to_owned()
                    },
                    upstream_provider: snapshot.upstream_provider.clone(),
                    upstream_dialect: snapshot.upstream_dialect.clone(),
                    transport_key: snapshot.transport_key.clone(),
                    capability_generation: generation,
                };
                let entry = self
                    .by_endpoint_model
                    .entry((endpoint.to_owned(), public_model.to_owned()))
                    .or_default();
                entry.retain(|existing| existing.member_id != candidate.member_id);
                entry.push(candidate);
            }
        }
        for candidates in self.by_endpoint_model.values_mut() {
            candidates.sort_by(|left, right| left.member_id.cmp(&right.member_id));
            candidates.dedup_by(|left, right| left.member_id == right.member_id);
        }
    }

    fn enabled_rules_for(&self, endpoint: &str, public_model: &str) -> Vec<&ModelRouteRule> {
        let endpoint = endpoint.trim();
        let model = public_model.trim();
        let mut rules: Vec<&ModelRouteRule> = self
            .rules
            .iter()
            .filter(|rule| {
                rule.enabled
                    && rule.public_model.trim() == model
                    && rule.endpoint_family.trim() == endpoint
            })
            .collect();
        rules.sort_by(|left, right| {
            left.priority
                .cmp(&right.priority)
                .then(left.id.cmp(&right.id))
        });
        rules
    }

    fn snapshots_for(&self, endpoint: &str, public_model: &str) -> Vec<MemberCapabilitySnapshot> {
        self.capability_snapshots()
            .into_iter()
            .filter(|snapshot| {
                snapshot.endpoint.trim() == endpoint.trim()
                    && snapshot.public_model.trim() == public_model.trim()
            })
            .collect()
    }

    fn explain_ambiguous(&self, endpoint: &str, public_model: &str) -> RouteRejectionReason {
        let endpoint = endpoint.trim();
        let model = public_model.trim();
        let matching: Vec<&ModelRouteRule> = self
            .rules
            .iter()
            .filter(|rule| {
                rule.public_model.trim() == model && rule.endpoint_family.trim() == endpoint
            })
            .collect();
        if matching.iter().any(|rule| rule.enabled) {
            return RouteRejectionReason::AmbiguousNoRule;
        }
        if matching.iter().any(|rule| !rule.enabled) {
            return RouteRejectionReason::LaneDisabled;
        }
        RouteRejectionReason::AmbiguousNoRule
    }

    fn explain_unknown(&self, endpoint: &str, public_model: &str) -> RouteRejectionReason {
        let snapshots = self.snapshots_for(endpoint, public_model);
        if snapshots.is_empty() {
            return RouteRejectionReason::UnknownModel;
        }
        let enabled_rules = self.enabled_rules_for(endpoint, public_model);
        if enabled_rules.is_empty() {
            return RouteRejectionReason::UnknownModel;
        }
        let any_match = snapshots.iter().any(|snapshot| {
            enabled_rules
                .iter()
                .any(|rule| rule_matches_snapshot(rule, snapshot))
        });
        if any_match {
            RouteRejectionReason::UnknownModel
        } else {
            RouteRejectionReason::NoMatchingRule
        }
    }

    /// Last successful member snapshots, used so a partial rebuild cannot
    /// empty the pool index.
    pub fn capability_snapshots(&self) -> Vec<MemberCapabilitySnapshot> {
        let mut snapshots = Vec::new();
        for ((endpoint, model), candidates) in &self.by_endpoint_model {
            for candidate in candidates {
                snapshots.push(MemberCapabilitySnapshot {
                    member_id: candidate.member_id.clone(),
                    public_model: model.clone(),
                    endpoint: endpoint.clone(),
                    upstream_provider: candidate.upstream_provider.clone(),
                    upstream_dialect: candidate.upstream_dialect.clone(),
                    upstream_model: candidate.upstream_model.clone(),
                    upstream_endpoint: candidate.upstream_endpoint.clone(),
                    transport_key: candidate.transport_key.clone(),
                    capability: MemberCapability::Supported,
                });
            }
        }
        snapshots
    }
}

fn distinct_providers(candidates: &[DispatchCandidate]) -> BTreeSet<&str> {
    candidates
        .iter()
        .map(|candidate| candidate.upstream_provider.as_str())
        .collect()
}

fn filter_candidates_by_rules(
    candidates: &[DispatchCandidate],
    rules: &[&ModelRouteRule],
) -> Vec<DispatchCandidate> {
    candidates
        .iter()
        .filter(|candidate| {
            rules
                .iter()
                .any(|rule| rule_matches_candidate(rule, candidate))
        })
        .cloned()
        .collect()
}

fn rule_matches_candidate(rule: &ModelRouteRule, candidate: &DispatchCandidate) -> bool {
    rule.upstream_provider.trim() == candidate.upstream_provider.trim()
        && rule.upstream_dialect.trim() == candidate.upstream_dialect.trim()
        && rule.upstream_model.trim() == candidate.upstream_model.trim()
}

fn rule_matches_snapshot(rule: &ModelRouteRule, snapshot: &MemberCapabilitySnapshot) -> bool {
    rule.upstream_provider.trim() == snapshot.upstream_provider.trim()
        && rule.upstream_dialect.trim() == snapshot.upstream_dialect.trim()
        && rule.upstream_model.trim() == snapshot.upstream_model.trim()
}

fn candidates_from_snapshots(snapshots: &[MemberCapabilitySnapshot]) -> Vec<DispatchCandidate> {
    snapshots
        .iter()
        .map(|snapshot| DispatchCandidate {
            member_id: snapshot.member_id.clone(),
            upstream_endpoint: snapshot.upstream_endpoint.clone(),
            upstream_model: snapshot.upstream_model.clone(),
            upstream_provider: snapshot.upstream_provider.clone(),
            upstream_dialect: snapshot.upstream_dialect.clone(),
            transport_key: snapshot.transport_key.clone(),
            capability_generation: 0,
        })
        .collect()
}

fn lane_key(candidate: &DispatchCandidate) -> (&str, &str) {
    (
        candidate.upstream_provider.as_str(),
        candidate.upstream_dialect.as_str(),
    )
}

fn candidate_matches_id(candidate: &DispatchCandidate, id: &str) -> bool {
    let id = id.trim();
    if id.is_empty() {
        return false;
    }
    candidate.member_id == id || id.ends_with(&format!(":{}", candidate.member_id))
}

fn member_excluded(candidate: &DispatchCandidate, excluded_member_ids: &[String]) -> bool {
    excluded_member_ids
        .iter()
        .any(|excluded| candidate_matches_id(candidate, excluded))
}

fn lane_priority(lane: (&str, &str), rules: &[&ModelRouteRule]) -> i64 {
    rules
        .iter()
        .filter(|rule| rule.lane_key() == lane)
        .map(|rule| rule.priority)
        .min()
        .unwrap_or(i64::MAX)
}

fn lane_groups<'a>(lane: (&str, &str), rules: &[&'a ModelRouteRule]) -> BTreeSet<&'a str> {
    rules
        .iter()
        .filter(|rule| rule.lane_key() == lane)
        .filter_map(|rule| rule.normalized_equivalent_group())
        .collect()
}

fn lanes_equivalent(left: (&str, &str), right: (&str, &str), rules: &[&ModelRouteRule]) -> bool {
    if left == right {
        return true;
    }
    let left_groups = lane_groups(left, rules);
    if left_groups.is_empty() {
        return false;
    }
    !left_groups.is_disjoint(&lane_groups(right, rules))
}

fn choose_lane(
    remaining: &[&DispatchCandidate],
    rules: &[&ModelRouteRule],
    last_lane: Option<(&str, &str)>,
) -> (String, String) {
    let mut ranked: Vec<(&str, &str)> = remaining
        .iter()
        .map(|candidate| lane_key(candidate))
        .collect();
    ranked.sort_by(|left, right| {
        lane_priority(*left, rules)
            .cmp(&lane_priority(*right, rules))
            .then(left.cmp(right))
    });
    ranked.dedup();
    if let Some(last) = last_lane {
        if ranked.iter().any(|lane| *lane == last) {
            return (last.0.to_owned(), last.1.to_owned());
        }
        if let Some(next) = ranked
            .iter()
            .copied()
            .find(|lane| lanes_equivalent(last, *lane, rules))
        {
            return (next.0.to_owned(), next.1.to_owned());
        }
        // Last lane exhausted and remaining lanes are not equivalent: no
        // cross-provider hop. Return last so the caller sees an empty set
        // after filtering remaining to that lane.
        return (last.0.to_owned(), last.1.to_owned());
    }
    let first = ranked.first().expect("remaining is non-empty");
    (first.0.to_owned(), first.1.to_owned())
}

/// One member's listed models used to build a production index.
///
/// Unknown / empty ids stay out. `snapshot_ok = false` keeps that member's
/// last successful snapshots so a partial refresh cannot empty the pool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberListing {
    pub member_id: String,
    pub listed_models: Vec<String>,
    pub upstream_provider: String,
    pub upstream_dialect: String,
    pub upstream_endpoint: String,
    pub transport_key: String,
    pub snapshot_ok: bool,
}

/// Build [`EffectiveRouteIndex`] from member mapping listings.
pub fn index_from_member_listings(
    route_id: impl Into<String>,
    generation: u64,
    endpoint: &str,
    members: &[MemberListing],
    prior: Option<&[MemberCapabilitySnapshot]>,
) -> EffectiveRouteIndex {
    let mut snapshots = Vec::new();
    for member in members {
        if !member.snapshot_ok {
            if let Some(prior) = prior {
                snapshots.extend(
                    prior
                        .iter()
                        .filter(|snapshot| snapshot.member_id == member.member_id)
                        .cloned(),
                );
            }
            continue;
        }
        if member.member_id.trim().is_empty() {
            continue;
        }
        for model in &member.listed_models {
            let model = model.trim();
            if model.is_empty() {
                continue;
            }
            snapshots.push(MemberCapabilitySnapshot {
                member_id: member.member_id.clone(),
                public_model: model.to_owned(),
                endpoint: endpoint.to_owned(),
                upstream_provider: member.upstream_provider.clone(),
                upstream_dialect: member.upstream_dialect.clone(),
                upstream_model: model.to_owned(),
                upstream_endpoint: member.upstream_endpoint.clone(),
                transport_key: member.transport_key.clone(),
                capability: MemberCapability::Supported,
            });
        }
    }
    EffectiveRouteIndex::build(route_id, generation, &snapshots)
}
