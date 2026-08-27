//! Runtime policy for Codex↔Grok pair adapters (flags + capability matrix).

use crate::bridge::protocol::pair::{
    dialect_compatibility, explicit_transparent_relay, sanitizer_allows_transparent,
    DialectCompatibility, PairDirection, ResponsesDialect,
};
use crate::models::{listed_model_matches, AdapterSourceProduct, AgentId, LOCAL_BRIDGE_EDGES};

use super::gateway::EdgeState;
use super::surface::DownstreamSurface;
use super::transport::UpstreamChannel;

pub(super) fn pair_direction(state: &EdgeState, channel: UpstreamChannel) -> Option<PairDirection> {
    let downstream = ResponsesDialect::from_agent(state.mapping_target?)?;
    let upstream = responses_dialect_from_channel(channel)?;
    PairDirection::from_dialects(downstream, upstream)
}

pub(super) fn pair_adapter_active(state: &EdgeState, channel: UpstreamChannel) -> bool {
    let Some(direction) = pair_direction(state, channel) else {
        return false;
    };
    if !flag_on(state, direction) {
        return false;
    }
    pair_edge_can_apply(state.mapping_source, state.mapping_target)
}

pub(super) fn identity_relay(
    channel: UpstreamChannel,
    surface: DownstreamSurface,
    state: &EdgeState,
) -> bool {
    if !channel.passthrough_for(surface) {
        return false;
    }
    if surface != DownstreamSurface::Responses {
        return true;
    }
    let Some(direction) = pair_direction(state, channel) else {
        return true;
    };
    if !pair_adapter_active(state, channel) {
        return true;
    }
    explicit_transparent_relay(direction.downstream(), direction.upstream())
        && dialect_compatibility(direction.downstream(), direction.upstream())
            == DialectCompatibility::Transparent
        && sanitizer_allows_transparent(&serde_json::Value::Null)
}

pub(super) fn pair_model_servable(state: &EdgeState, model: &str) -> bool {
    let model = model.trim();
    if state.listed_models.is_empty() {
        return false;
    }
    if model.is_empty() {
        return state
            .upstream
            .model
            .as_deref()
            .map(str::trim)
            .is_some_and(|configured| {
                !configured.is_empty()
                    && state
                        .listed_models
                        .iter()
                        .any(|item| listed_model_matches(item, configured))
            });
    }
    state
        .listed_models
        .iter()
        .any(|item| listed_model_matches(item, model))
}

pub(super) fn pair_edge_can_apply(
    source: Option<AdapterSourceProduct>,
    target: Option<AgentId>,
) -> bool {
    let (Some(source), Some(target)) = (source, target) else {
        return false;
    };
    LOCAL_BRIDGE_EDGES
        .iter()
        .any(|edge| edge.source == source && edge.target == target && edge.can_apply)
}

fn flag_on(state: &EdgeState, direction: PairDirection) -> bool {
    match direction {
        PairDirection::CodexIngressGrokUpstream => state.codex_ingress_grok_upstream,
        PairDirection::GrokIngressCodexUpstream => state.grok_ingress_codex_upstream,
    }
}

fn responses_dialect_from_channel(channel: UpstreamChannel) -> Option<ResponsesDialect> {
    match channel {
        UpstreamChannel::CodexResponses => Some(ResponsesDialect::Codex),
        UpstreamChannel::Grok => Some(ResponsesDialect::Grok),
        UpstreamChannel::OpenAiChat | UpstreamChannel::Anthropic => None,
    }
}

impl ResponsesDialect {
    fn from_agent(agent: AgentId) -> Option<Self> {
        match agent {
            AgentId::Codex => Some(Self::Codex),
            AgentId::Grok => Some(Self::Grok),
            _ => None,
        }
    }
}
