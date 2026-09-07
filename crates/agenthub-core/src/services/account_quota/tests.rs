use super::*;
use crate::models::AccountKind;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;

fn make_jwt(claims: Value) -> String {
    let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"none"}"#);
    let payload = URL_SAFE_NO_PAD.encode(claims.to_string().as_bytes());
    format!("{header}.{payload}.sig")
}

#[test]
fn parse_wham_classifies_by_window_length_not_primary_name() {
    let now = Utc::now();
    // Real Codex/ChatGPT shape: primary = 7d, secondary = 5h.
    let body = json!({
        "plan_type": "prolite",
        "rate_limit": {
            "primary_window": {
                "used_percent": 18.0,
                "limit_window_seconds": 604800,
                "reset_after_seconds": 86400
            },
            "secondary_window": {
                "used_percent": 42.5,
                "limit_window_seconds": 18000,
                "reset_after_seconds": 7200
            }
        }
    });
    let snap = parse_openai_wham_usage(&body, now);
    assert_eq!(
        snap.quota5h_pct,
        Some(42.5),
        "5h must come from secondary (18000s)"
    );
    assert_eq!(
        snap.quota7d_pct,
        Some(18.0),
        "7d must come from primary (604800s)"
    );
    assert_eq!(snap.plan_type.as_deref(), Some("prolite"));
    let r5 = snap.reset_5h_at.expect("5h reset");
    let r7 = snap.reset_7d_at.expect("7d reset");
    // reset_after is relative to probe `now`, clamped to window
    assert!(((r5 - now).num_seconds() - 7200).abs() <= 1);
    assert!(((r7 - now).num_seconds() - 86400).abs() <= 1);
    let label = snap.reset_in_label(now).unwrap();
    assert!(label.contains("后重置"));
    // 5h row uses only 5h reset (~2h), never weekly remaining.
    assert!(label.starts_with("2h"), "label={label}");
    let label7 = snap.reset_in_label_7d(now).unwrap();
    assert!(
        label7.starts_with("1d")
            || label7.contains("24h")
            || label7.starts_with("1d0h")
            || label7.contains("后重置")
    );
    // 86400s = 1d exactly
    assert!(label7.starts_with("1d") || label7 == "24h00m 后重置" || label7.starts_with("1d0h"));
}

#[test]
fn seven_day_remaining_never_exceeds_window() {
    let now = Utc::now();
    // Malicious/wrong reset_after of 9 days on a 7d window must clamp.
    let raw = CodexRawSnapshot {
        primary_used: Some(10.0),
        primary_reset_after: Some(9 * 24 * 3600), // 9 days
        primary_window_mins: Some(10080),         // 7 days
        secondary_used: Some(1.0),
        secondary_reset_after: Some(10 * 3600), // 10h on 5h window → clamp to 5h
        secondary_window_mins: Some(300),
        ..Default::default()
    };
    let snap = normalize_codex_snapshot_to_quota(&raw, now, "test");
    let rem7 = (snap.reset_7d_at.unwrap() - now).num_seconds();
    assert!(
        rem7 <= 7 * 24 * 3600 + 120,
        "7d remaining {rem7}s must not exceed 7d window"
    );
    assert!(
        rem7 >= 7 * 24 * 3600 - 5,
        "should clamp near full 7d, got {rem7}"
    );
    let rem5 = (snap.reset_5h_at.unwrap() - now).num_seconds();
    assert!(
        rem5 <= 5 * 3600 + 120,
        "5h remaining {rem5}s must not exceed 5h window"
    );
}

#[test]
fn parse_wham_without_window_length_uses_openai_primary_as_7d() {
    let now = Utc::now();
    let body = json!({
        "rate_limit": {
            "primary_window": { "used_percent": 10.0, "reset_after_seconds": 1000 },
            "secondary_window": { "used_percent": 20.0, "reset_after_seconds": 500 }
        }
    });
    let snap = parse_openai_wham_usage(&body, now);
    assert_eq!(snap.quota7d_pct, Some(10.0));
    assert_eq!(snap.quota5h_pct, Some(20.0));
}

#[test]
fn parse_grok_billing_weekly_and_monthly() {
    let now = DateTime::parse_from_rfc3339("2026-07-10T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let weekly = json!({
        "config": {
            "currentPeriod": {
                "type": "WEEKLY",
                "start": "2026-07-09T03:25:00Z",
                "end": "2026-07-16T03:25:00Z"
            },
            "creditUsagePercent": 12.5,
            "productUsage": [{ "product": "Api", "usagePercent": 12.5 }]
        }
    });
    let monthly = json!({
        "config": {
            "monthlyLimit": { "val": 15000 },
            "used": { "val": 1500 },
            "billingPeriodStart": "2026-07-01T00:00:00Z",
            "billingPeriodEnd": "2026-08-01T00:00:00Z"
        }
    });
    let snap = parse_grok_billing(Some(&weekly), Some(&monthly), now);
    assert_eq!(snap.quota7d_pct, Some(12.5));
    assert_eq!(snap.plan_type.as_deref(), Some("SuperGrok"));
    let r7 = snap.reset_7d_at.expect("weekly period end");
    // end - now = ~6d3h, within 7d cap
    let rem = (r7 - now).num_seconds();
    assert!(rem > 5 * 24 * 3600);
    assert!(rem <= 7 * 24 * 3600 + 120);
    assert!(snap.reset_in_label_7d(now).unwrap().contains("后重置"));
    // 5h unused for Grok billing
    assert!(snap.quota5h_pct.is_none());
    assert!(snap.reset_in_label(now).is_none());
}

#[test]
fn parse_grok_billing_monthly_only() {
    let now = Utc::now();
    let monthly = json!({
        "config": {
            "monthlyLimit": { "val": 15000 },
            "used": { "val": 7500 },
            "billingPeriodEnd": (now + ChronoDuration::days(10)).to_rfc3339()
        }
    });
    let snap = parse_grok_billing(None, Some(&monthly), now);
    assert_eq!(snap.quota7d_pct, Some(50.0));
    assert_eq!(snap.plan_type.as_deref(), Some("SuperGrok"));
    // monthly remaining clamped to 7d for the 7d bar
    let rem = (snap.reset_7d_at.unwrap() - now).num_seconds();
    assert!(rem <= 7 * 24 * 3600 + 120);
}

#[test]
fn parse_grok_billing_period_only_keeps_zero_percent_bar() {
    let now = DateTime::parse_from_rfc3339("2026-08-24T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let weekly = json!({
        "config": {
            "currentPeriod": {
                "type": "USAGE_PERIOD_TYPE_WEEKLY",
                "start": "2026-08-23T18:42:45Z",
                "end": "2026-08-30T18:42:45Z"
            },
            "isUnifiedBillingUser": true,
            "onDemandCap": { "val": 0 },
            "onDemandUsed": { "val": 0 }
        }
    });
    let snap = parse_grok_billing(Some(&weekly), None, now);
    assert_eq!(snap.quota7d_pct, Some(0.0));
    assert!(snap.reset_7d_at.is_some());
    assert!(!snap.is_empty());
}

#[test]
fn parse_grok_billing_monthly_usage_total_used() {
    let now = Utc::now();
    let monthly = json!({
        "config": {
            "monthlyLimit": { "val": 60000 },
            "usage": {
                "includedUsed": { "val": 12000 },
                "totalUsed": { "val": 15000 }
            },
            "billingCycle": {
                "billingPeriodEnd": (now + ChronoDuration::days(4)).to_rfc3339()
            }
        }
    });
    let snap = parse_grok_billing(None, Some(&monthly), now);
    assert_eq!(snap.quota7d_pct, Some(25.0));
    assert_eq!(snap.plan_type.as_deref(), Some("plan $600"));
    assert!(snap.reset_7d_at.is_some());
}

#[test]
fn parse_grok_billing_prefers_monthly_percent_over_period_zero() {
    let now = Utc::now();
    let weekly = json!({
        "config": {
            "currentPeriod": {
                "type": "USAGE_PERIOD_TYPE_WEEKLY",
                "end": (now + ChronoDuration::days(2)).to_rfc3339()
            }
        }
    });
    let monthly = json!({
        "config": {
            "monthlyLimit": { "val": 15000 },
            "used": { "val": 3000 }
        }
    });
    let snap = parse_grok_billing(Some(&weekly), Some(&monthly), now);
    assert_eq!(snap.quota7d_pct, Some(20.0));
}

#[test]
fn parse_kiro_usage_limits_reads_credit_row() {
    let now = Utc::now();
    let body = json!({
        "nextDateReset": 1_790_812_800.0,
        "subscriptionInfo": { "subscriptionTitle": "KIRO FREE" },
        "usageBreakdownList": [{
            "resourceType": "CREDIT",
            "currentUsageWithPrecision": 0.29,
            "usageLimitWithPrecision": 50.0,
            "nextDateReset": 1_790_812_800.0
        }]
    });
    let snap = parse_kiro_usage_limits(&body, now);
    assert_eq!(snap.credit_used, Some(0.29));
    assert_eq!(snap.credit_limit, Some(50.0));
    assert_eq!(snap.plan_type.as_deref(), Some("KIRO FREE"));
    assert_eq!(
        snap.credit_reset_at.map(|t| t.timestamp()),
        Some(1_790_812_800)
    );
    assert!(snap.quota5h_pct.is_none());
    assert!(snap.quota7d_pct.is_none());
    assert!(!snap.is_empty());
}

#[test]
fn apply_quota_snapshot_writes_kiro_credits() {
    let now = Utc::now();
    let mut acc = Account {
        id: "k1".into(),
        agent_id: AgentId::Kiro,
        kind: AccountKind::Oauth,
        label: "x".into(),
        credentials: json!({}),
        extra: json!({}),
        status: "active".into(),
        is_current: true,
        created_at: "t".into(),
        updated_at: "t".into(),
    };
    let snap = QuotaSnapshot {
        credit_used: Some(0.29),
        credit_limit: Some(50.0),
        credit_reset_at: DateTime::from_timestamp(1_790_812_800, 0),
        plan_type: Some("KIRO FREE".into()),
        source: "kiro_get_usage_limits",
        ..Default::default()
    };
    assert!(apply_quota_snapshot(&mut acc, &snap, now));
    assert_eq!(
        acc.extra.get("creditUsed").and_then(|v| v.as_f64()),
        Some(0.29)
    );
    assert_eq!(
        acc.extra.get("creditLimit").and_then(|v| v.as_f64()),
        Some(50.0)
    );
    assert_eq!(
        acc.extra.get("subscription").and_then(|v| v.as_str()),
        Some("KIRO FREE")
    );
}

#[test]
fn codex_token_expiry_ignores_short_id_token() {
    // Real Codex shape: access exp ~10d, id_token exp ~1h (often already past).
    let access_exp = (Utc::now() + ChronoDuration::hours(200)).timestamp();
    let id_exp = (Utc::now() - ChronoDuration::hours(100)).timestamp();
    let access = {
        use base64::engine::general_purpose::URL_SAFE_NO_PAD;
        use base64::Engine;
        let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"none"}"#);
        let payload =
            URL_SAFE_NO_PAD.encode(json!({"sub":"u","exp": access_exp}).to_string().as_bytes());
        format!("{header}.{payload}.sig")
    };
    let id_token = {
        use base64::engine::general_purpose::URL_SAFE_NO_PAD;
        use base64::Engine;
        let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"none"}"#);
        let payload =
            URL_SAFE_NO_PAD.encode(json!({"sub":"u","exp": id_exp}).to_string().as_bytes());
        format!("{header}.{payload}.sig")
    };
    let mut acc = Account {
        id: "c1".into(),
        agent_id: AgentId::Codex,
        kind: AccountKind::Oauth,
        label: "x".into(),
        credentials: json!({
            "format": "auth_json",
            "body": { "tokens": { "access_token": access, "id_token": id_token } },
            // Stale value from a previous id_token-based heal:
            "expires_at": DateTime::from_timestamp(id_exp, 0).unwrap().to_rfc3339(),
        }),
        extra: json!({
            "expiresAt": DateTime::from_timestamp(id_exp, 0).unwrap().to_rfc3339(),
            "tokenExpired": true,
        }),
        status: "active".into(),
        is_current: true,
        created_at: "t".into(),
        updated_at: "t".into(),
    };
    assert!(heal_token_expiry(&mut acc));
    assert_eq!(
        acc.extra.get("tokenExpired").and_then(|v| v.as_bool()),
        Some(false),
        "must use access_token exp, not expired id_token"
    );
    let exp = acc.extra.get("expiresAt").and_then(|v| v.as_str()).unwrap();
    let rem = (DateTime::parse_from_rfc3339(exp)
        .unwrap()
        .with_timezone(&Utc)
        - Utc::now())
    .num_seconds();
    assert!(
        rem > 100 * 3600,
        "remaining should be ~200h from access, got {rem}"
    );
}

#[test]
fn refresh_quota_reset_label_ticks_from_absolute_time() {
    let mut acc = Account {
        id: "c1".into(),
        agent_id: AgentId::Codex,
        kind: AccountKind::Oauth,
        label: "x".into(),
        credentials: json!({}),
        extra: json!({
            "quota5hPct": 10,
            "quota5hResetAt": (Utc::now() + ChronoDuration::hours(1) + ChronoDuration::minutes(5)).to_rfc3339(),
            "quotaResetIn": "9h00m 后重置"
        }),
        status: "active".into(),
        is_current: true,
        created_at: "t".into(),
        updated_at: "t".into(),
    };
    assert!(refresh_quota_reset_label(&mut acc, Utc::now()));
    let label = acc
        .extra
        .get("quotaResetIn")
        .and_then(|v| v.as_str())
        .unwrap();
    assert!(
        label.starts_with("1h"),
        "expected ~1h remaining, got {label}"
    );
    assert!(label.contains("后重置"));
}

#[test]
fn parse_claude_five_hour_seven_day() {
    let now = Utc::now();
    let reset5 = (now + ChronoDuration::hours(3)).to_rfc3339();
    let body = json!({
        "five_hour": { "utilization": 12.0, "resets_at": reset5 },
        "seven_day": { "utilization": 55.5, "resets_in_seconds": 100000 }
    });
    let snap = parse_claude_oauth_usage(&body, now);
    assert_eq!(snap.quota5h_pct, Some(12.0));
    assert_eq!(snap.quota7d_pct, Some(55.5));
    assert!(snap.reset_5h_at.is_some());
    assert!(snap.reset_7d_at.is_some());
}

#[test]
fn heal_token_expiry_from_jwt_exp() {
    let exp = (Utc::now() + ChronoDuration::hours(6)).timestamp();
    let access = make_jwt(json!({ "sub": "u1", "exp": exp }));
    let mut acc = Account {
        id: "c1".into(),
        agent_id: AgentId::Codex,
        kind: AccountKind::Oauth,
        label: "codex-oauth".into(),
        credentials: json!({
            "format": "auth_json",
            "body": { "tokens": { "access_token": access, "refresh_token": "rt" } }
        }),
        extra: json!({ "source": "live" }),
        status: "active".into(),
        is_current: true,
        created_at: "t".into(),
        updated_at: "t".into(),
    };
    assert!(heal_token_expiry(&mut acc));
    assert!(acc
        .extra
        .get("expiresAt")
        .and_then(|v| v.as_str())
        .is_some());
    assert_eq!(
        acc.extra.get("tokenExpired").and_then(|v| v.as_bool()),
        Some(false)
    );
    assert!(acc
        .credentials
        .get("access_token")
        .and_then(|v| v.as_str())
        .is_some());
}

#[test]
fn apply_snapshot_writes_ui_fields() {
    let mut acc = Account {
        id: "c1".into(),
        agent_id: AgentId::Codex,
        kind: AccountKind::Oauth,
        label: "x".into(),
        credentials: json!({}),
        extra: json!({}),
        status: "active".into(),
        is_current: true,
        created_at: "t".into(),
        updated_at: "t".into(),
    };
    let now = Utc::now();
    let snap = QuotaSnapshot {
        quota5h_pct: Some(62.4),
        quota7d_pct: Some(10.1),
        reset_5h_at: Some(now + ChronoDuration::hours(2)),
        reset_7d_at: None,
        plan_type: Some("plus".into()),
        source: "test",
        ..Default::default()
    };
    assert!(apply_quota_snapshot(&mut acc, &snap, now));
    assert_eq!(
        acc.extra.get("quota5hPct").and_then(|v| v.as_i64()),
        Some(62)
    );
    assert_eq!(
        acc.extra.get("quota7dPct").and_then(|v| v.as_i64()),
        Some(10)
    );
    assert!(acc
        .extra
        .get("quota5hResetAt")
        .and_then(|v| v.as_str())
        .is_some());
    assert!(acc
        .extra
        .get("quotaResetIn")
        .and_then(|v| v.as_str())
        .unwrap()
        .contains("后重置"));
    assert_eq!(
        acc.extra.get("subscription").and_then(|v| v.as_str()),
        Some("plus")
    );
    assert!(!quota_is_stale(&acc, now));
}

#[test]
fn apply_snapshot_clears_omitted_5h_when_upstream_is_weekly_only() {
    let mut acc = oauth_account_with_extra(json!({
        "quota5hPct": 40,
        "quota5hResetAt": "2026-08-01T00:00:00Z",
        "quotaResetIn": "即将重置",
        "quota7dPct": 10
    }));
    let now = Utc::now();
    let snap = QuotaSnapshot {
        quota5h_pct: None,
        quota7d_pct: Some(22.0),
        reset_5h_at: None,
        reset_7d_at: Some(now + ChronoDuration::days(2)),
        plan_type: None,
        source: "test",
        ..Default::default()
    };
    assert!(apply_quota_snapshot(&mut acc, &snap, now));
    assert!(acc.extra.get("quota5hPct").is_none());
    assert!(acc.extra.get("quotaResetIn").is_none());
    assert_eq!(
        acc.extra.get("quota7dPct").and_then(|v| v.as_i64()),
        Some(22)
    );
}

#[test]
fn codex_probe_payload_matches_sub2api_cheap_stream() {
    let payload = codex_responses_probe_payload();
    assert_eq!(payload["model"], "codex-auto-review");
    assert_eq!(payload["stream"], true);
    assert_eq!(payload["store"], false);
    assert!(payload
        .get("instructions")
        .and_then(|v| v.as_str())
        .is_some());
}

#[test]
fn parse_wham_keeps_top_level_pool_when_additional_codex_meter_is_empty() {
    let now = Utc::now();
    let body = json!({
        "plan_type": "plus",
        "rate_limit": {
            "primary_window": {
                "used_percent": 18.0,
                "limit_window_seconds": 604800,
                "reset_after_seconds": 86400
            },
            "secondary_window": {
                "used_percent": 42.5,
                "limit_window_seconds": 18000,
                "reset_after_seconds": 7200
            }
        },
        "additional_rate_limits": [{
            "metered_feature": "codex_bengalfox",
            "rate_limit": serde_json::Value::Null
        }]
    });
    let snap = parse_openai_wham_usage(&body, now);
    assert_eq!(snap.quota5h_pct, Some(42.5));
    assert_eq!(snap.quota7d_pct, Some(18.0));
}

#[test]
fn parse_wham_uses_bengalfox_only_when_shared_pool_has_no_windows() {
    let now = Utc::now();
    let body = json!({
        "rate_limit": serde_json::Value::Null,
        "additional_rate_limits": [{
            "metered_feature": "codex_bengalfox",
            "rate_limit": {
                "primary_window": {
                    "used_percent": 7.0,
                    "limit_window_seconds": 604800,
                    "reset_after_seconds": 1000
                },
                "secondary_window": {
                    "used_percent": 3.0,
                    "limit_window_seconds": 18000,
                    "reset_after_seconds": 500
                }
            }
        }]
    });
    let snap = parse_openai_wham_usage(&body, now);
    assert_eq!(snap.quota5h_pct, Some(3.0));
    assert_eq!(snap.quota7d_pct, Some(7.0));
}

fn oauth_account_with_extra(extra: Value) -> Account {
    Account {
        id: "c1".into(),
        agent_id: AgentId::Codex,
        kind: AccountKind::Oauth,
        label: "x".into(),
        credentials: json!({}),
        extra,
        status: "active".into(),
        is_current: true,
        created_at: "t".into(),
        updated_at: "t".into(),
    }
}

#[test]
fn quota_is_stale_when_updated_at_missing_even_if_pct_fields_exist() {
    let acc = oauth_account_with_extra(json!({
        "quota5hPct": 40,
        "quota7dPct": 10
    }));
    assert!(quota_is_stale(&acc, Utc::now()));
}

#[test]
fn quota_is_stale_when_5h_reset_has_elapsed() {
    let now = Utc::now();
    let acc = oauth_account_with_extra(json!({
        "quota5hPct": 40,
        "quotaUpdatedAt": now.to_rfc3339(),
        "quota5hResetAt": (now - ChronoDuration::seconds(1)).to_rfc3339()
    }));
    assert!(quota_is_stale(&acc, now));
}

#[test]
fn elapsed_5h_reset_hides_bar_instead_of_inventing_zero() {
    let now = Utc::now();
    let mut acc = oauth_account_with_extra(json!({
        "quota5hPct": 40,
        "quota5hResetAt": (now - ChronoDuration::seconds(1)).to_rfc3339(),
        "quotaResetIn": "即将重置",
        "quota7dPct": 10
    }));
    assert!(refresh_quota_reset_label(&mut acc, now));
    assert!(acc.extra.get("quota5hPct").is_none());
    assert!(acc.extra.get("quotaResetIn").is_none());
    assert_eq!(
        acc.extra.get("quota7dPct").and_then(|v| v.as_i64()),
        Some(10)
    );
}

fn pi_codex_account(credentials: Value, extra: Value) -> Account {
    Account {
        id: "pi-1".into(),
        agent_id: AgentId::Pi,
        kind: AccountKind::Oauth,
        label: "pi:openai-codex".into(),
        credentials,
        extra,
        status: "active".into(),
        is_current: true,
        created_at: "t".into(),
        updated_at: "t".into(),
    }
}

fn openai_account_id_jwt() -> String {
    make_jwt(json!({
        "email": "pi-codex@example.com",
        "https://api.openai.com/auth": { "chatgpt_account_id": "acc-pi" }
    }))
}

#[test]
fn extract_chatgpt_account_id_from_pi_extra_and_credentials() {
    let acc = pi_codex_account(
        json!({
            "format": "auth_json",
            "provider": "openai-codex",
            "access_token": "opaque"
        }),
        json!({ "provider": "openai-codex", "accountId": "acc-extra" }),
    );
    assert_eq!(
        extract_chatgpt_account_id(&acc).as_deref(),
        Some("acc-extra")
    );

    let acc = pi_codex_account(
        json!({
            "format": "auth_json",
            "provider": "openai-codex",
            "account_id": "acc-cred",
            "access_token": "opaque"
        }),
        json!({ "provider": "openai-codex" }),
    );
    assert_eq!(
        extract_chatgpt_account_id(&acc).as_deref(),
        Some("acc-cred")
    );
}

#[test]
fn extract_chatgpt_account_id_from_pi_codex_id_token() {
    let id_token = openai_account_id_jwt();
    let acc = pi_codex_account(
        json!({
            "format": "auth_json",
            "provider": "openai-codex",
            "access_token": "opaque",
            "id_token": id_token,
            "body": {
                "openai-codex": { "type": "oauth", "access": "opaque" }
            }
        }),
        json!({ "provider": "openai-codex" }),
    );
    assert_eq!(extract_chatgpt_account_id(&acc).as_deref(), Some("acc-pi"));
}

#[test]
fn extract_chatgpt_account_id_from_pi_nested_access_jwt() {
    let access = openai_account_id_jwt();
    let acc = pi_codex_account(
        json!({
            "format": "auth_json",
            "provider": "openai-codex",
            "body": {
                "openai-codex": { "type": "oauth", "access": access }
            }
        }),
        json!({ "provider": "openai-codex" }),
    );
    assert_eq!(extract_chatgpt_account_id(&acc).as_deref(), Some("acc-pi"));
}
