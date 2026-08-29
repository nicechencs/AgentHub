//! Insert-time billing conversion for parsed usage events.
//!
//! Priority is unchanged: Grok invoice ticks / candidate table / log costUSD / $0.
//! `UsageService::recompute_stored_costs` stays on the facade (needs registry + repo).

use crate::models::{AgentId, ParsedUsageEvent};
use crate::usage::grok::{grok_model_has_pricing, pricing_candidates};
use crate::usage::{
    estimate_cost_usd_for_agent_at, has_embedded_pricing, has_embedded_pricing_for, CostTokens,
};

pub fn event_missing_pricing(ev: &ParsedUsageEvent) -> bool {
    if ev.agent_id == AgentId::Grok {
        !grok_model_has_pricing(&ev.model)
    } else {
        !has_embedded_pricing_for(ev.agent_id, &ev.model, Some(&ev.ts))
    }
}

pub fn cost_for_event(ev: &ParsedUsageEvent) -> f64 {
    if ev.agent_id == AgentId::Grok && ev.cost_usd.is_none() {
        if let Some(model) = pricing_candidates(&ev.model)
            .into_iter()
            .find(|c| has_embedded_pricing(c))
        {
            return estimate_cost_usd_for_agent_at(
                ev.agent_id,
                &model,
                CostTokens {
                    input: ev.input_tokens,
                    output: ev.output_tokens,
                    cache_create: ev.cache_creation_tokens,
                    cache_create_1h: ev.cache_creation_1h_tokens,
                    cache_read: ev.cache_read_tokens,
                    fast: ev.fast,
                },
                None,
                Some(&ev.ts),
            );
        }
    }
    estimate_cost_usd_for_agent_at(
        ev.agent_id,
        &ev.model,
        CostTokens {
            input: ev.input_tokens,
            output: ev.output_tokens,
            cache_create: ev.cache_creation_tokens,
            cache_create_1h: ev.cache_creation_1h_tokens,
            cache_read: ev.cache_read_tokens,
            fast: ev.fast,
        },
        ev.cost_usd,
        Some(&ev.ts),
    )
}
