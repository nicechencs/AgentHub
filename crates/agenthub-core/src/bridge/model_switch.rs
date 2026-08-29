//! Request-scoped edge pick after local-bearer auth and body model are known.
//!
//! This is not [`crate::services::ProviderService`] switch, ticket bind, or
//! [`crate::services::AdapterRouteService::plan`]. AccountPicker is same-class
//! failover, not used here. Static mapping tables stay in
//! [`crate::models::adapter_model_mapping`].

use crate::models::{
    find_adapter_model_mapping, is_openrouter_backup_model, listed_model_matches, map_edge_model,
    mapping_table_is_active, AdapterModelMapResult, AdapterSourceProduct, AgentId,
};

use super::runtime::DownstreamResponsesProfile;

/// One running (or known) edge the model-switch helper can pick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ModelSwitchCandidate {
    pub profile_id: String,
    pub source: AdapterSourceProduct,
    pub target: AgentId,
    /// Only edges with the same explicit downstream Responses profile may
    /// switch requests. `None` is the generic/non-Responses host-test value.
    pub downstream_responses_profile: Option<DownstreamResponsesProfile>,
    pub custom_openai_compat: bool,
    /// Same local surface as the authenticated lead. Cross-surface is never switched.
    pub same_surface: bool,
    pub running: bool,
    /// Models this edge advertises on GET /v1/models. A hit stays on the lead
    /// even when the mapping table is reserved-empty.
    pub listed_models: Vec<String>,
}

/// Per-request pick after the lead EdgeState is authenticated and the body model is known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ModelSwitchDecision {
    /// Stay on the authenticated lead.
    Stay,
    /// Use this other running edge for this request only.
    SwitchTo { profile_id: String },
    /// Lead cannot map the model, and no running alternate can serve it.
    Unavailable,
}

/// After gateway auth, if the lead mapping is Missing and another running
/// edge can serve the model (Mapped or Passthrough), switch for this request.
/// AccountPicker is not used here — that is same-class failover, not cross-vendor.
pub(crate) fn decide_model_switch(
    lead: &ModelSwitchCandidate,
    model: &str,
    others: &[ModelSwitchCandidate],
) -> ModelSwitchDecision {
    let lead_result = map_edge_model(lead.source, lead.target, model, lead.custom_openai_compat);
    if lead_serves(lead, model, lead_result) {
        return ModelSwitchDecision::Stay;
    }

    let mut capable_running: Option<&ModelSwitchCandidate> = None;
    for candidate in others {
        if candidate.profile_id == lead.profile_id {
            continue;
        }
        if candidate.target != lead.target
            || !candidate.same_surface
            || candidate.downstream_responses_profile != lead.downstream_responses_profile
        {
            continue;
        }
        let result = map_edge_model(
            candidate.source,
            candidate.target,
            model,
            candidate.custom_openai_compat,
        );
        if !matches!(
            result,
            AdapterModelMapResult::Mapped(_) | AdapterModelMapResult::Passthrough
        ) {
            continue;
        }
        if candidate.running && capable_running.is_none() {
            capable_running = Some(candidate);
        }
    }

    if let Some(alternate) = capable_running {
        return ModelSwitchDecision::SwitchTo {
            profile_id: alternate.profile_id.clone(),
        };
    }
    ModelSwitchDecision::Unavailable
}

fn lead_serves(lead: &ModelSwitchCandidate, model: &str, result: AdapterModelMapResult) -> bool {
    match result {
        AdapterModelMapResult::Mapped(_) | AdapterModelMapResult::Passthrough => true,
        AdapterModelMapResult::Missing => {
            let needle = model.trim();
            if !needle.is_empty()
                && lead
                    .listed_models
                    .iter()
                    .any(|listed| listed_model_matches(listed, needle))
            {
                return true;
            }
            if lead.custom_openai_compat && lead.listed_models.is_empty() {
                return true;
            }
            if lead.custom_openai_compat && is_openrouter_backup_model(needle) {
                return true;
            }
            find_adapter_model_mapping(lead.source, lead.target)
                .is_none_or(|table| !mapping_table_is_active(table))
                && lead.listed_models.is_empty()
        }
    }
}

#[cfg(test)]
mod tests;
