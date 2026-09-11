use super::*;
use crate::models::AgentId;

#[test]
fn prefers_log_cost_usd() {
    let c = estimate_cost_usd("claude-sonnet-4", 1_000_000, 0, 0, 0, Some(1.0));
    assert!((c - 1.0).abs() < 0.01);
}

#[test]
fn embedded_sonnet_exact() {
    assert!(has_embedded_pricing("claude-sonnet-4"));
    // $3 / 1M input; 100k stays below the 200k long-context tier.
    let c = estimate_cost_usd("claude-sonnet-4", 100_000, 0, 0, 0, None);
    assert!((c - 0.3).abs() < 0.01, "got {c}");
}

#[test]
fn small_row_not_rounded_to_zero() {
    // Cheap model + high cache: must not collapse to $0.00 at 2-dp storage.
    let c = estimate_cost_usd_for_agent(
        AgentId::Codex,
        "gpt-5.6-luna",
        CostTokens::from_parts(192_028, 62, 0, 191_232),
        None,
    );
    assert!(c > 0.001, "got {c}");
}

#[test]
fn dated_model_alias() {
    assert!(has_embedded_pricing("claude-sonnet-4-20250514"));
    let a = rates_for("claude-sonnet-4");
    let b = rates_for("claude-sonnet-4-20250514");
    assert!((a.input - b.input).abs() < 1e-12);
}

#[test]
fn kimi_for_coding_in_table() {
    assert!(has_embedded_pricing("kimi-for-coding"));
    assert!(has_embedded_pricing("moonshot/kimi-k2.6"));
}

#[test]
fn official_publishers_outside_legacy_families_are_priced() {
    assert!(has_embedded_pricing("deepseek-chat"));
    assert!(has_embedded_pricing("deepseek-v4-flash"));
    assert!(has_embedded_pricing("deepseek-flash"));
    assert!(has_embedded_pricing("zai/glm-4.7"));
    assert!(has_embedded_pricing("glm-4.7"));
    assert!(has_embedded_pricing("qwen-plus"));
    let flash = rates_for("deepseek-flash");
    let v4 = rates_for("deepseek-v4-flash");
    assert!((flash.input - v4.input).abs() < 1e-12);
    assert!((flash.output - v4.output).abs() < 1e-12);
    let glm = rates_for("glm-4.7");
    let zai = rates_for("zai/glm-4.7");
    assert!((glm.input - zai.input).abs() < 1e-12);
}

#[test]
fn cache_read_cheaper_than_create_for_sonnet() {
    let r = rates_for("claude-sonnet-4");
    assert!(r.cache_read < r.cache_create);
    assert!(r.cache_read < r.input);
}

#[test]
fn unknown_model_costs_zero_without_log_cost() {
    assert!(!has_embedded_pricing("totally-unknown-model-xyz"));
    let c = estimate_cost_usd(
        "totally-unknown-model-xyz",
        1_000_000,
        1_000_000,
        0,
        0,
        None,
    );
    assert!((c - 0.0).abs() < 1e-12, "got {c}");
}

#[test]
fn unknown_model_still_prefers_log_cost() {
    let c = estimate_cost_usd("totally-unknown-model-xyz", 1, 1, 0, 0, Some(2.5));
    assert!((c - 2.5).abs() < 0.01);
}

#[test]
fn codex_billable_tokens_trusts_stored_non_cached_layout() {
    // Stored layout is already non-cached — never peel again.
    // full=1000, cache=250 → stored input=750; peel would wrongly yield 500.
    assert_eq!(codex_billable_tokens(750, 250), (750, 250));
    assert_eq!(codex_billable_tokens(100_000, 900_000), (100_000, 900_000));
    assert_eq!(codex_billable_tokens(500, 0), (500, 0));
    // Must be stable across repeated passes (old heuristic eroded toward 0).
    let mut input = 750i64;
    let cache = 250i64;
    for _ in 0..5 {
        let (b, c) = codex_billable_tokens(input, cache);
        assert_eq!((b, c), (750, 250));
        input = b;
    }
}

#[test]
fn codex_cost_on_normalized_tokens() {
    // gpt-5.6-luna short: $0.2 / $1.2 / $0.02 per 1M — 100k billable, under 272K context
    let c = estimate_cost_usd_for_agent(
        AgentId::Codex,
        "gpt-5.6-luna",
        CostTokens::from_parts(100_000, 0, 0, 50_000),
        None,
    );
    // 0.1M * 0.2 + 0.05M * 0.02 = 0.021
    assert!((c - 0.021).abs() < 0.001, "got {c}");
}

#[test]
fn cache_creation_1h_bills_at_twice_input() {
    // 100k 1h-cache writes on sonnet-4: 2 × $3/1M = $0.60 (not the 5m create rate).
    let c = estimate_cost_from_tokens(
        "claude-sonnet-4",
        CostTokens {
            cache_create_1h: 100_000,
            ..CostTokens::default()
        },
        None,
    );
    assert!((c - 0.6).abs() < 0.01, "got {c}");
}

#[test]
fn sonnet_45_above_200k_is_marginal() {
    // LiteLLM above_200k on sonnet-4-5: first 200K at $3, rest at $6.
    let c = estimate_cost_usd("claude-sonnet-4-5", 1_000_000, 0, 0, 0, None);
    assert!((c - 5.4).abs() < 0.01, "got {c}");
    let c6 = estimate_cost_usd("claude-sonnet-4-6", 1_000_000, 0, 0, 0, None);
    assert!((c6 - 5.4).abs() < 0.01, "got {c6}");
}

#[test]
fn gpt_56_sol_whole_request_switches_at_272k() {
    let long = estimate_cost_from_tokens(
        "gpt-5.6-sol",
        CostTokens::from_parts(300_000, 1_000, 0, 100),
        None,
    );
    // Whole request at long rates $8/$30/$0.8 per 1M.
    // 0.3*8 + 0.001*30 + 0.0001*0.8 = 2.43008
    assert!(
        (long - 2.43008).abs() < 1e-5,
        "long-context cost was {long}"
    );

    let short = estimate_cost_from_tokens(
        "gpt-5.6-sol",
        CostTokens::from_parts(100_000, 1_000, 0, 100),
        None,
    );
    // Short rates $4/$20/$0.4: 0.4 + 0.02 + 0.00004 = 0.42004
    assert!(
        (short - 0.42004).abs() < 1e-5,
        "short-context cost was {short}"
    );
}

#[test]
fn cached_context_selects_long_context_tier() {
    let c = estimate_cost_from_tokens(
        "gpt-5.6-luna",
        CostTokens::from_parts(10_000, 1_000, 0, 500_000),
        None,
    );
    // 510K context > 272K → long $0.4/$1.8/$0.04 per 1M
    // 0.01*0.4 + 0.001*1.8 + 0.5*0.04 = 0.0258
    assert!((c - 0.0258).abs() < 1e-5, "cached-heavy cost was {c}");
}

#[test]
fn auto_review_is_not_a_priced_model() {
    assert!(!has_embedded_pricing("codex-auto-review"));
    assert_eq!(
        pricing_model_for(AgentId::Codex, "codex-auto-review", None),
        "gpt-5.6-luna"
    );
    assert_eq!(
        pricing_model_for(
            AgentId::Codex,
            "codex-auto-review",
            Some("2026-07-29T23:59:59Z")
        ),
        "gpt-5.4"
    );
    assert_eq!(
        pricing_model_for(
            AgentId::Codex,
            "codex-auto-review",
            Some("2026-07-30T00:00:00Z")
        ),
        "gpt-5.6-luna"
    );
    assert_eq!(
        pricing_model_for(AgentId::Codex, "gpt-5.6-sol", None),
        "gpt-5.6-sol"
    );
}

#[test]
fn auto_review_bills_at_published_backend_rates() {
    // Stay under the 272K whole-request switch so this isolates the rate card.
    let tokens = CostTokens::from_parts(100_000, 0, 0, 0);
    let luna = estimate_cost_usd_for_agent(AgentId::Codex, "gpt-5.6-luna", tokens, None);
    let review = estimate_cost_usd_for_agent_at(
        AgentId::Codex,
        "codex-auto-review",
        tokens,
        None,
        Some("2026-08-26T00:00:00Z"),
    );
    assert!((review - luna).abs() < 1e-12, "got {review} want {luna}");
    let old = estimate_cost_usd_for_agent_at(
        AgentId::Codex,
        "codex-auto-review",
        tokens,
        None,
        Some("2026-07-01T00:00:00Z"),
    );
    let gpt54 = estimate_cost_usd_for_agent(AgentId::Codex, "gpt-5.4", tokens, None);
    assert!((old - gpt54).abs() < 1e-12, "got {old} want {gpt54}");
    assert!((review - 0.02).abs() < 1e-9, "luna input got {review}");
    assert!((old - 0.25).abs() < 1e-9, "gpt-5.4 input got {old}");
}

#[test]
fn fast_multiplier_applies_to_codex_token_cost() {
    // Stay under the 272K whole-request switch so this isolates Fast.
    let standard = estimate_cost_usd_for_agent(
        AgentId::Codex,
        "gpt-5.6-sol",
        CostTokens::from_parts(100_000, 0, 0, 0),
        None,
    );
    let fast = estimate_cost_usd_for_agent(
        AgentId::Codex,
        "gpt-5.6-sol",
        CostTokens {
            input: 100_000,
            fast: true,
            ..CostTokens::default()
        },
        None,
    );
    assert!((standard - 0.4).abs() < 0.01, "standard got {standard}");
    assert!((fast - 0.8).abs() < 0.01, "fast got {fast}");
}

#[test]
fn log_cost_skips_fast_and_long_context() {
    let c = estimate_cost_usd_for_agent(
        AgentId::Codex,
        "gpt-5.6-sol",
        CostTokens {
            input: 1_000_000,
            fast: true,
            ..CostTokens::default()
        },
        Some(1.5),
    );
    assert!((c - 1.5).abs() < 0.01);
}
