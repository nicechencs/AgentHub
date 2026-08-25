use serde_json::{Map, Value};

use super::{
    apply_claude_live_model_env, claude_context_window_choice, claude_context_window_for,
    listed_model_matches, parse_claude_context_window_override, strip_claude_context_marker,
    window_from_context_marker, CLAUDE_CODE_AUTO_COMPACT_WINDOW, CLAUDE_CODE_MAX_CONTEXT_TOKENS,
    CLAUDE_WINDOW_1M, CLAUDE_WINDOW_200K,
};

#[test]
fn strips_1m_marker_and_matches_listed_ids() {
    assert_eq!(
        strip_claude_context_marker("stealth/ox-alpha[1m]"),
        "stealth/ox-alpha"
    );
    assert_eq!(
        strip_claude_context_marker("stealth/ox-alpha[1M]"),
        "stealth/ox-alpha"
    );
    assert_eq!(
        strip_claude_context_marker("  stealth/ox-alpha  "),
        "stealth/ox-alpha"
    );
    assert!(listed_model_matches(
        "stealth/ox-alpha",
        "stealth/ox-alpha[1m]"
    ));
    assert!(listed_model_matches(
        "STEALTH/OX-ALPHA[1M]",
        "stealth/ox-alpha"
    ));
    assert!(!listed_model_matches("stealth/ox-alpha", "gpt-4o"));
}

#[test]
fn window_comes_from_override_or_1m_marker_never_from_model_id() {
    assert_eq!(
        window_from_context_marker("any/model[1m]"),
        Some(CLAUDE_WINDOW_1M)
    );
    assert_eq!(claude_context_window_for("stealth/ox-alpha", None), None);
    assert_eq!(claude_context_window_for("custom/unknown", None), None);
    assert_eq!(claude_context_window_for("claude-sonnet-4", None), None);
    assert_eq!(
        claude_context_window_for("stealth/ox-alpha", Some(CLAUDE_WINDOW_1M)),
        Some(CLAUDE_WINDOW_1M)
    );
    assert_eq!(
        claude_context_window_for("any/id[1m]", None),
        Some(CLAUDE_WINDOW_1M)
    );
}

#[test]
fn live_env_pins_model_roles_and_writes_window_only_when_declared() {
    let mut env = Map::new();
    apply_claude_live_model_env(&mut env, "stealth/ox-alpha", None);
    assert_eq!(
        env.get("ANTHROPIC_MODEL"),
        Some(&Value::String("stealth/ox-alpha".into()))
    );
    assert_eq!(
        env.get("ANTHROPIC_DEFAULT_OPUS_MODEL"),
        Some(&Value::String("stealth/ox-alpha".into()))
    );
    assert!(env.get(CLAUDE_CODE_MAX_CONTEXT_TOKENS).is_none());

    apply_claude_live_model_env(&mut env, "stealth/ox-alpha", Some(CLAUDE_WINDOW_1M));
    assert_eq!(
        env.get(CLAUDE_CODE_MAX_CONTEXT_TOKENS),
        Some(&Value::String(CLAUDE_WINDOW_1M.to_string()))
    );
    assert_eq!(
        env.get(CLAUDE_CODE_AUTO_COMPACT_WINDOW),
        Some(&Value::String(CLAUDE_WINDOW_1M.to_string()))
    );

    apply_claude_live_model_env(&mut env, "claude-sonnet-4", None);
    assert_eq!(
        env.get("ANTHROPIC_MODEL"),
        Some(&Value::String("claude-sonnet-4".into()))
    );
    assert!(env.get(CLAUDE_CODE_MAX_CONTEXT_TOKENS).is_none());

    apply_claude_live_model_env(&mut env, "", None);
    assert!(env.get("ANTHROPIC_MODEL").is_none());
    assert!(env.get(CLAUDE_CODE_MAX_CONTEXT_TOKENS).is_none());
}

#[test]
fn context_window_choice_maps_known_values_and_aliases() {
    assert_eq!(parse_claude_context_window_override(""), None);
    assert_eq!(parse_claude_context_window_override("auto"), None);
    assert_eq!(
        parse_claude_context_window_override("200000"),
        Some(CLAUDE_WINDOW_200K)
    );
    assert_eq!(
        parse_claude_context_window_override("1048576"),
        Some(CLAUDE_WINDOW_1M)
    );
    assert_eq!(
        parse_claude_context_window_override("1000000"),
        Some(CLAUDE_WINDOW_1M)
    );
    assert_eq!(claude_context_window_choice(""), "auto");
    assert_eq!(claude_context_window_choice("200000"), "200000");
    assert_eq!(claude_context_window_choice("1048576"), "1048576");
    assert_eq!(claude_context_window_choice("1000000"), "1048576");
    assert_eq!(claude_context_window_choice("128000"), "auto");
}
