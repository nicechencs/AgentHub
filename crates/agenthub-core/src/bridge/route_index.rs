//! Shared resolver snapshot for v2 route pools.
//!
//! `resolve` and `list_models` read the same map. Absent index keeps v1 lead
//! + `switch_edge_for_model`; a present index fail-closes unknown and
//! ambiguous models and does not scan sibling profiles.

use std::collections::{BTreeMap, BTreeSet};

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

/// `(route, endpoint, public_model) → DispatchCandidate[]` built from member
/// snapshots. `/models` enumerates the same keys.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveRouteIndex {
    pub route_id: String,
    pub generation: u64,
    by_endpoint_model: BTreeMap<(String, String), Vec<DispatchCandidate>>,
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
        }
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
        let mut providers = BTreeSet::new();
        for candidate in candidates {
            providers.insert(candidate.upstream_provider.as_str());
        }
        if providers.len() > 1 {
            return Err(RouteResolveError::AmbiguousModel);
        }
        Ok(candidates.clone())
    }

    /// Public model ids that currently have at least one supported candidate.
    pub fn list_models(&self, endpoint: &str) -> Vec<String> {
        let endpoint = endpoint.trim();
        let mut models = Vec::new();
        for ((snap_endpoint, model), candidates) in &self.by_endpoint_model {
            if snap_endpoint != endpoint || candidates.is_empty() {
                continue;
            }
            let mut providers = BTreeSet::new();
            for candidate in candidates {
                providers.insert(candidate.upstream_provider.as_str());
            }
            if providers.len() > 1 {
                continue;
            }
            models.push(model.clone());
        }
        models
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

/// Build [`EffectiveRouteIndex`] from member mapping listings. This is the
/// production snapshot path used by start / restore; tests must not bypass it
/// with a hand-rolled index when asserting that path.
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
