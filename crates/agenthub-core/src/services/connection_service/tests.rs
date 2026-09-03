//! ConnectionService tests (separate from production module).

use crate::models::{Account, AccountKind, AgentId, ConnectionTrashKind, Provider, ProviderInput};
use crate::services::{AccountService, ConnectionService, ProviderService};
use crate::storage::{
    AccountRepo, ActiveBindingRepo, ActiveBindingRow, ConnectionTrashRepo, Database, ProviderRepo,
};
use serde_json::json;

fn seed_extension_fields(
    db: &Database,
    agent_key: &str,
    model_id: Option<&str>,
    profile_id: Option<&str>,
) {
    let repo = ActiveBindingRepo::new(db.clone());
    let existing = repo.get(agent_key).unwrap();
    let row = ActiveBindingRow {
        agent_key: agent_key.into(),
        account_id: existing.as_ref().and_then(|r| r.account_id.clone()),
        provider_id: existing.as_ref().and_then(|r| r.provider_id.clone()),
        model_id: model_id.map(|s| s.into()),
        config_profile_id: profile_id.map(|s| s.into()),
        revision: existing.as_ref().map(|r| r.revision).unwrap_or(1),
        created_at: existing
            .as_ref()
            .map(|r| r.created_at.clone())
            .unwrap_or_else(|| "2026-01-01 00:00:00.000000".into()),
        updated_at: "2026-01-01 00:00:00.000000".into(),
    };
    repo.upsert(&row).unwrap();
}

fn tmp() -> (tempfile::TempDir, Database) {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("c.db")).unwrap();
    (dir, db)
}

fn account(id: &str, agent: AgentId, current: bool, updated: &str) -> Account {
    Account {
        id: id.into(),
        agent_id: agent,
        kind: AccountKind::ApiKey,
        label: id.into(),
        credentials: json!({ "key": "x" }),
        extra: json!({}),
        status: "active".into(),
        is_current: current,
        created_at: updated.into(),
        updated_at: updated.into(),
    }
}

fn provider(id: &str, agent: AgentId, current: bool, updated: &str) -> Provider {
    Provider {
        id: id.into(),
        agent_id: agent,
        name: id.into(),
        settings_config: json!({}),
        meta: json!({}),
        is_current: current,
        created_at: updated.into(),
        updated_at: updated.into(),
    }
}

fn provider_input(id: &str, agent: AgentId, current: bool) -> ProviderInput {
    ProviderInput {
        id: id.into(),
        agent_id: agent,
        name: id.into(),
        settings_config: json!({ "k": id }),
        meta: json!({}),
        is_current: current,
    }
}

#[test]
fn activate_account_sets_current_and_binding() {
    let (_d, db) = tmp();
    let accounts = AccountRepo::new(db.clone());
    let providers = ProviderRepo::new(db.clone());
    let a = accounts
        .create(&account("acc1", AgentId::Claude, false, "t1"))
        .unwrap();
    providers
        .create(&provider("prov1", AgentId::Claude, true, "t0"))
        .unwrap();
    let conn = ConnectionService::new(db.clone());
    let (switched, b) = conn
        .activate_account(AgentId::Claude, &a.id, "t1", "t2")
        .unwrap();
    assert!(switched.is_current);
    assert_eq!(b.account_id.as_deref(), Some("acc1"));
    assert!(b.provider_id.is_none());
    assert!(providers.get_current(AgentId::Claude).unwrap().is_none());
    let got = conn.get_active(AgentId::Claude).unwrap().unwrap();
    assert_eq!(got.account_id.as_deref(), Some("acc1"));
}

#[test]
fn activate_provider_clears_account_current() {
    let (_d, db) = tmp();
    let accounts = AccountRepo::new(db.clone());
    let providers = ProviderRepo::new(db.clone());
    accounts
        .create(&account("acc1", AgentId::Codex, true, "t1"))
        .unwrap();
    let p = providers
        .create(&provider("prov1", AgentId::Codex, false, "t2"))
        .unwrap();
    let conn = ConnectionService::new(db);
    let (switched, b) = conn
        .activate_provider(AgentId::Codex, &p.id, "t2", "t3")
        .unwrap();
    assert!(switched.is_current);
    assert_eq!(b.provider_id.as_deref(), Some("prov1"));
    assert!(b.account_id.is_none());
    assert!(accounts.get_current(AgentId::Codex).unwrap().is_none());
}

#[test]
fn create_current_account_demotes_provider_and_sets_binding() {
    let (_d, db) = tmp();
    let providers = ProviderService::new(db.clone());
    let accounts = AccountService::new(db.clone());
    let conn = ConnectionService::new(db.clone());

    providers
        .create(&provider_input("p1", AgentId::Claude, true))
        .unwrap();
    assert!(conn
        .get_active(AgentId::Claude)
        .unwrap()
        .unwrap()
        .provider_id
        .is_some());

    let created = accounts
        .create(crate::models::AccountInput {
            agent_id: AgentId::Claude,
            kind: AccountKind::ApiKey,
            label: "acc-current".into(),
            credentials: json!({ "format": "api_key", "api_key": "sk" }),
            extra: json!({}),
            is_current: true,
        })
        .unwrap();
    assert!(created.is_current);

    let binding = conn.get_active(AgentId::Claude).unwrap().unwrap();
    assert_eq!(binding.account_id.as_deref(), Some(created.id.as_str()));
    assert!(binding.provider_id.is_none());
    assert!(ProviderRepo::new(db.clone())
        .get_current(AgentId::Claude)
        .unwrap()
        .is_none());
    assert!(AccountRepo::new(db)
        .get_current(AgentId::Claude)
        .unwrap()
        .is_some());
}

#[test]
fn create_current_provider_demotes_account_and_sets_binding() {
    let (_d, db) = tmp();
    let accounts = AccountService::new(db.clone());
    let providers = ProviderService::new(db.clone());
    let conn = ConnectionService::new(db.clone());

    accounts
        .create(crate::models::AccountInput {
            agent_id: AgentId::Codex,
            kind: AccountKind::ApiKey,
            label: "acc".into(),
            credentials: json!({ "format": "api_key", "api_key": "sk" }),
            extra: json!({}),
            is_current: true,
        })
        .unwrap();

    let created = providers
        .create(&provider_input("p-new", AgentId::Codex, true))
        .unwrap();
    assert!(created.is_current);

    let binding = conn.get_active(AgentId::Codex).unwrap().unwrap();
    assert_eq!(binding.provider_id.as_deref(), Some("p-new"));
    assert!(binding.account_id.is_none());
    assert!(AccountRepo::new(db)
        .get_current(AgentId::Codex)
        .unwrap()
        .is_none());
}

#[test]
fn upsert_current_provider_activates_atomically() {
    let (_d, db) = tmp();
    let providers = ProviderService::new(db.clone());
    let accounts = AccountService::new(db.clone());
    let conn = ConnectionService::new(db.clone());

    accounts
        .create(crate::models::AccountInput {
            agent_id: AgentId::Grok,
            kind: AccountKind::ApiKey,
            label: "a".into(),
            credentials: json!({ "k": 1 }),
            extra: json!({}),
            is_current: true,
        })
        .unwrap();

    providers
        .upsert(&provider_input("up1", AgentId::Grok, true))
        .unwrap();
    let b1 = conn.get_active(AgentId::Grok).unwrap().unwrap();
    assert_eq!(b1.provider_id.as_deref(), Some("up1"));

    let mut input = provider_input("up1", AgentId::Grok, true);
    input.name = "renamed".into();
    providers.upsert(&input).unwrap();
    let b2 = conn.get_active(AgentId::Grok).unwrap().unwrap();
    assert_eq!(b2.provider_id.as_deref(), Some("up1"));
    assert!(b2.revision >= b1.revision);
    assert!(AccountRepo::new(db)
        .get_current(AgentId::Grok)
        .unwrap()
        .is_none());
}

#[test]
fn update_current_provider_to_non_current_clears_binding() {
    let (_d, db) = tmp();
    let providers = ProviderService::new(db.clone());
    let conn = ConnectionService::new(db.clone());
    providers
        .create(&provider_input("p-cur", AgentId::Claude, true))
        .unwrap();
    assert!(conn.get_active(AgentId::Claude).unwrap().is_some());

    let demoted = providers
        .update(&provider_input("p-cur", AgentId::Claude, false))
        .unwrap();
    assert!(!demoted.is_current);
    assert!(conn.get_active(AgentId::Claude).unwrap().is_none());
    assert!(ProviderRepo::new(db)
        .get_current(AgentId::Claude)
        .unwrap()
        .is_none());
}

#[test]
fn upsert_current_provider_to_non_current_clears_binding() {
    let (_d, db) = tmp();
    let providers = ProviderService::new(db.clone());
    let conn = ConnectionService::new(db.clone());
    providers
        .create(&provider_input("p-up", AgentId::Codex, true))
        .unwrap();
    assert_eq!(
        conn.get_active(AgentId::Codex)
            .unwrap()
            .unwrap()
            .provider_id
            .as_deref(),
        Some("p-up")
    );

    let demoted = providers
        .upsert(&provider_input("p-up", AgentId::Codex, false))
        .unwrap();
    assert!(!demoted.is_current);
    assert!(conn.get_active(AgentId::Codex).unwrap().is_none());
}

#[test]
fn update_non_current_provider_does_not_touch_other_binding() {
    let (_d, db) = tmp();
    let providers = ProviderService::new(db.clone());
    let conn = ConnectionService::new(db.clone());
    providers
        .create(&provider_input("p-active", AgentId::Grok, true))
        .unwrap();
    providers
        .create(&provider_input("p-other", AgentId::Grok, false))
        .unwrap();
    let before = conn.get_active(AgentId::Grok).unwrap().unwrap();

    let mut input = provider_input("p-other", AgentId::Grok, false);
    input.name = "renamed-other".into();
    providers.update(&input).unwrap();

    let after = conn.get_active(AgentId::Grok).unwrap().unwrap();
    assert_eq!(after.provider_id, before.provider_id);
    assert_eq!(after.revision, before.revision);
    assert!(
        providers
            .get("p-active", Some(AgentId::Grok))
            .unwrap()
            .is_current
    );
}

#[test]
fn binding_write_failure_rolls_back_create_current_account() {
    let (_d, db) = tmp();
    let conn = ConnectionService::new(db.clone());
    db.with_conn(|c| {
        c.execute_batch(
            r#"
            CREATE TRIGGER fail_binding_write
            BEFORE INSERT ON agent_active_bindings
            BEGIN
                SELECT RAISE(ABORT, 'injected binding write failure');
            END;
            "#,
        )?;
        Ok(())
    })
    .unwrap();

    let err = conn
        .create_and_activate_account(&account("acc-x", AgentId::Claude, true, "t1"))
        .unwrap_err();
    assert_eq!(err.code(), "db");
    assert!(AccountRepo::new(db.clone())
        .get_by_id("acc-x")
        .unwrap()
        .is_none());
    assert!(ActiveBindingRepo::new(db).get("claude").unwrap().is_none());
}

#[test]
fn binding_write_failure_rolls_back_cross_type_demotion() {
    let (_d, db) = tmp();
    let accounts = AccountRepo::new(db.clone());
    let providers = ProviderRepo::new(db.clone());
    let conn = ConnectionService::new(db.clone());

    providers
        .create(&provider("p-hold", AgentId::Claude, true, "t0"))
        .unwrap();
    conn.activate_provider(AgentId::Claude, "p-hold", "t0", "t0b")
        .unwrap();
    let before_binding = conn.get_active(AgentId::Claude).unwrap().unwrap();
    let before_provider = providers.get_current(AgentId::Claude).unwrap().unwrap();

    db.with_conn(|c| {
        c.execute_batch(
            r#"
            CREATE TRIGGER fail_binding_update
            BEFORE UPDATE ON agent_active_bindings
            BEGIN
                SELECT RAISE(ABORT, 'injected binding update failure');
            END;
            "#,
        )?;
        Ok(())
    })
    .unwrap();

    let err = conn
        .create_and_activate_account(&account("acc-fail", AgentId::Claude, true, "t1"))
        .unwrap_err();
    assert_eq!(err.code(), "db");

    assert!(accounts.get_by_id("acc-fail").unwrap().is_none());
    let after_provider = providers.get_current(AgentId::Claude).unwrap().unwrap();
    assert_eq!(after_provider.id, before_provider.id);
    assert!(after_provider.is_current);
    let after_binding = conn.get_active(AgentId::Claude).unwrap().unwrap();
    assert_eq!(after_binding.provider_id, before_binding.provider_id);
    assert_eq!(after_binding.revision, before_binding.revision);
}

#[test]
fn update_and_activate_account_rejects_stale_revision() {
    let (_d, db) = tmp();
    let conn = ConnectionService::new(db.clone());
    let created = conn
        .create_and_activate_account(&account("acc-cas", AgentId::Claude, true, "t1"))
        .unwrap()
        .0;
    let mut next = created.clone();
    next.label = "newer".into();
    next.updated_at = "t2".into();
    conn.update_and_activate_account(&next, "t1").unwrap();

    next.label = "stale".into();
    next.updated_at = "t3".into();
    let err = conn.update_and_activate_account(&next, "t1").unwrap_err();
    assert_eq!(err.code(), "account.merge.conflict");
    let stored = AccountRepo::new(db).get_by_id("acc-cas").unwrap().unwrap();
    assert_eq!(stored.label, "newer");
}

#[test]
fn clear_clears_binding_and_legacy_currents_without_backfill() {
    let (_d, db) = tmp();
    let accounts = AccountRepo::new(db.clone());
    accounts
        .create(&account("a", AgentId::Kimi, true, "t1"))
        .unwrap();
    let conn = ConnectionService::new(db.clone());
    conn.activate_account(AgentId::Kimi, "a", "t1", "t2")
        .unwrap();
    assert!(conn.get_active(AgentId::Kimi).unwrap().is_some());

    conn.clear(AgentId::Kimi).unwrap();

    assert!(conn.get_active(AgentId::Kimi).unwrap().is_none());
    assert!(accounts.get_current(AgentId::Kimi).unwrap().is_none());
    assert!(ProviderRepo::new(db.clone())
        .get_current(AgentId::Kimi)
        .unwrap()
        .is_none());
    assert!(conn.get_active(AgentId::Kimi).unwrap().is_none());
    assert!(ActiveBindingRepo::new(db).get("kimi").unwrap().is_none());
}

#[test]
fn delete_active_account_clears_binding() {
    let (_d, db) = tmp();
    let accounts = AccountService::new(db.clone());
    let conn = ConnectionService::new(db.clone());
    let created = accounts
        .create(crate::models::AccountInput {
            agent_id: AgentId::Claude,
            kind: AccountKind::ApiKey,
            label: "to-delete".into(),
            credentials: json!({ "k": 1 }),
            extra: json!({}),
            is_current: true,
        })
        .unwrap();
    assert!(conn.get_active(AgentId::Claude).unwrap().is_some());

    accounts.delete(&created.id, AgentId::Claude).unwrap();
    assert!(conn.get_active(AgentId::Claude).unwrap().is_none());
    assert!(ActiveBindingRepo::new(db).get("claude").unwrap().is_none());
}

#[test]
fn delete_active_provider_clears_binding() {
    let (_d, db) = tmp();
    let providers = ProviderService::new(db.clone());
    let conn = ConnectionService::new(db.clone());
    providers
        .create(&provider_input("p-del", AgentId::Codex, true))
        .unwrap();
    assert_eq!(
        conn.get_active(AgentId::Codex)
            .unwrap()
            .unwrap()
            .provider_id
            .as_deref(),
        Some("p-del")
    );

    providers.delete("p-del", AgentId::Codex).unwrap();
    assert!(conn.get_active(AgentId::Codex).unwrap().is_none());
}

#[test]
fn delete_non_active_row_keeps_binding() {
    let (_d, db) = tmp();
    let providers = ProviderService::new(db.clone());
    let conn = ConnectionService::new(db.clone());
    providers
        .create(&provider_input("p-active", AgentId::Grok, true))
        .unwrap();
    providers
        .create(&provider_input("p-other", AgentId::Grok, false))
        .unwrap();

    providers.delete("p-other", AgentId::Grok).unwrap();
    let binding = conn.get_active(AgentId::Grok).unwrap().unwrap();
    assert_eq!(binding.provider_id.as_deref(), Some("p-active"));
    assert!(
        providers
            .get("p-active", Some(AgentId::Grok))
            .unwrap()
            .is_current
    );
}

#[test]
fn dangling_binding_is_cleared_and_not_returned() {
    let (_d, db) = tmp();
    let conn = ConnectionService::new(db.clone());
    let bindings = ActiveBindingRepo::new(db.clone());
    bindings
        .set_refs(
            "claude",
            Some("missing-acc".into()),
            None,
            None,
            "2020-01-01T00:00:00Z",
        )
        .unwrap();

    let active = conn.get_active(AgentId::Claude).unwrap();
    assert!(active.is_none(), "dangling binding must not be returned");
    assert!(bindings.get("claude").unwrap().is_none());
}

#[test]
fn dangling_binding_repairs_from_legacy_current_when_available() {
    let (_d, db) = tmp();
    let accounts = AccountRepo::new(db.clone());
    let conn = ConnectionService::new(db.clone());
    let bindings = ActiveBindingRepo::new(db.clone());

    accounts
        .create(&account("real", AgentId::Claude, true, "t1"))
        .unwrap();
    bindings
        .set_refs(
            "claude",
            Some("ghost".into()),
            None,
            None,
            "2020-01-01T00:00:00Z",
        )
        .unwrap();

    let active = conn.get_active(AgentId::Claude).unwrap().unwrap();
    assert_eq!(active.account_id.as_deref(), Some("real"));
    assert!(active.provider_id.is_none());
    assert!(accounts.get_by_id("real").unwrap().unwrap().is_current);
}

#[test]
fn dual_legacy_current_without_binding_resolved_to_single() {
    let (_d, db) = tmp();
    let accounts = AccountRepo::new(db.clone());
    let providers = ProviderRepo::new(db.clone());
    let conn = ConnectionService::new(db.clone());

    accounts
        .create(&account("acc", AgentId::Claude, true, "t2"))
        .unwrap();
    providers
        .create(&provider("prov", AgentId::Claude, false, "t1"))
        .unwrap();
    // Force dual current (legacy dirty state, no binding row).
    db.with_conn(|c| {
        c.execute("UPDATE providers SET is_current = 1 WHERE id = 'prov'", [])?;
        Ok(())
    })
    .unwrap();

    let active = conn.get_active(AgentId::Claude).unwrap().unwrap();
    assert_eq!(active.account_id.as_deref(), Some("acc"));
    assert!(active.provider_id.is_none());
    assert!(
        accounts
            .get_current(AgentId::Claude)
            .unwrap()
            .unwrap()
            .is_current
    );
    assert!(providers.get_current(AgentId::Claude).unwrap().is_none());
    // Only one current account.
    assert_eq!(
        accounts
            .list(Some(AgentId::Claude))
            .unwrap()
            .into_iter()
            .filter(|a| a.is_current)
            .count(),
        1
    );
}

#[test]
fn multi_same_type_legacy_currents_resolved_deterministically() {
    let (_d, db) = tmp();
    let accounts = AccountRepo::new(db.clone());
    let conn = ConnectionService::new(db.clone());

    accounts
        .create(&account("old", AgentId::Grok, true, "2020-01-01T00:00:00Z"))
        .unwrap();
    accounts
        .create(&account(
            "new",
            AgentId::Grok,
            false,
            "2024-01-01T00:00:00Z",
        ))
        .unwrap();
    // Force two account currents without binding.
    db.with_conn(|c| {
        c.execute(
            "UPDATE accounts SET is_current = 1 WHERE agent_id = 'grok'",
            [],
        )?;
        Ok(())
    })
    .unwrap();

    let active = conn.get_active(AgentId::Grok).unwrap().unwrap();
    assert_eq!(active.account_id.as_deref(), Some("new"));
    let currents: Vec<_> = accounts
        .list(Some(AgentId::Grok))
        .unwrap()
        .into_iter()
        .filter(|a| a.is_current)
        .map(|a| a.id)
        .collect();
    assert_eq!(currents, vec!["new".to_string()]);
}

#[test]
fn valid_binding_overrides_wrong_legacy_flags() {
    let (_d, db) = tmp();
    let accounts = AccountRepo::new(db.clone());
    let providers = ProviderRepo::new(db.clone());
    let conn = ConnectionService::new(db.clone());
    let bindings = ActiveBindingRepo::new(db.clone());

    accounts
        .create(&account("acc", AgentId::Claude, false, "t1"))
        .unwrap();
    providers
        .create(&provider("prov", AgentId::Claude, true, "t2"))
        .unwrap();
    // Binding says account; legacy current says provider.
    bindings
        .set_refs("claude", Some("acc".into()), None, None, "t3")
        .unwrap();

    let active = conn.get_active(AgentId::Claude).unwrap().unwrap();
    assert_eq!(active.account_id.as_deref(), Some("acc"));
    assert!(active.provider_id.is_none());
    assert!(accounts.get_by_id("acc").unwrap().unwrap().is_current);
    assert!(!providers.get_by_id("prov").unwrap().unwrap().is_current);
}

#[test]
fn get_active_never_returns_binding_for_non_current_without_repair() {
    let (_d, db) = tmp();
    let accounts = AccountRepo::new(db.clone());
    let conn = ConnectionService::new(db.clone());
    let bindings = ActiveBindingRepo::new(db.clone());

    accounts
        .create(&account("acc", AgentId::Claude, false, "t1"))
        .unwrap();
    bindings
        .set_refs("claude", Some("acc".into()), None, None, "t2")
        .unwrap();

    let active = conn.get_active(AgentId::Claude).unwrap().unwrap();
    assert_eq!(active.account_id.as_deref(), Some("acc"));
    assert!(accounts.get_by_id("acc").unwrap().unwrap().is_current);
}

#[test]
fn activate_revision_conflict_rolls_back() {
    let (_d, db) = tmp();
    let accounts = AccountRepo::new(db.clone());
    let providers = ProviderRepo::new(db.clone());
    let conn = ConnectionService::new(db.clone());

    accounts
        .create(&account("acc1", AgentId::Claude, true, "t1"))
        .unwrap();
    providers
        .create(&provider("p1", AgentId::Claude, false, "t2"))
        .unwrap();
    conn.activate_account(AgentId::Claude, "acc1", "t1", "t1b")
        .unwrap();
    let before = conn.get_active(AgentId::Claude).unwrap().unwrap();

    let err = conn
        .activate_provider(AgentId::Claude, "p1", "wrong-revision", "t3")
        .unwrap_err();
    assert_eq!(err.code(), "provider.state");

    assert!(
        accounts
            .get_current(AgentId::Claude)
            .unwrap()
            .unwrap()
            .is_current
    );
    assert!(providers.get_current(AgentId::Claude).unwrap().is_none());
    let after = conn.get_active(AgentId::Claude).unwrap().unwrap();
    assert_eq!(after.account_id, before.account_id);
    assert_eq!(after.revision, before.revision);
}

#[test]
fn non_current_create_does_not_touch_binding() {
    let (_d, db) = tmp();
    let providers = ProviderService::new(db.clone());
    let conn = ConnectionService::new(db.clone());
    providers
        .create(&provider_input("p-cur", AgentId::Claude, true))
        .unwrap();
    let before = conn.get_active(AgentId::Claude).unwrap().unwrap();

    providers
        .create(&provider_input("p-other", AgentId::Claude, false))
        .unwrap();
    let after = conn.get_active(AgentId::Claude).unwrap().unwrap();
    assert_eq!(after.provider_id, before.provider_id);
    assert_eq!(after.revision, before.revision);
}

#[test]
fn lazy_backfill_prefers_newer_account() {
    let (_d, db) = tmp();
    let accounts = AccountRepo::new(db.clone());
    accounts
        .create(&account("old", AgentId::Grok, true, "2020-01-01T00:00:00Z"))
        .unwrap();
    accounts
        .create(&account("new", AgentId::Grok, true, "2024-01-01T00:00:00Z"))
        .unwrap();
    let conn = ConnectionService::new(db);
    let b = conn.get_active(AgentId::Grok).unwrap().unwrap();
    assert_eq!(b.account_id.as_deref(), Some("new"));
}

#[test]
fn record_account_active_delegates_to_activate_not_binding_only() {
    let (_d, db) = tmp();
    let accounts = AccountRepo::new(db.clone());
    let providers = ProviderRepo::new(db.clone());
    accounts
        .create(&account("a1", AgentId::Claude, false, "t1"))
        .unwrap();
    providers
        .create(&provider("p1", AgentId::Claude, true, "t0"))
        .unwrap();
    let conn = ConnectionService::new(db);
    let b = conn.record_account_active(AgentId::Claude, "a1").unwrap();
    assert_eq!(b.account_id.as_deref(), Some("a1"));
    // Must also demote provider current (full activate path).
    assert!(providers.get_current(AgentId::Claude).unwrap().is_none());
    assert!(accounts.get_by_id("a1").unwrap().unwrap().is_current);
}

#[test]
fn unknown_account_rejected() {
    let (_d, db) = tmp();
    let conn = ConnectionService::new(db);
    let err = conn
        .record_account_active(AgentId::Claude, "missing")
        .unwrap_err();
    assert_eq!(err.code(), "not_found");
}

#[test]
fn no_public_binding_only_write_api_on_connection_service() {
    // API surface check: production writers go through activate / create / demote paths.
    // record_account_active is cfg(test)+pub(crate) and delegates to activate (not binding-only).
    let (_d, db) = tmp();
    let svc = ConnectionService::new(db);
    let _ = ConnectionService::activate_account
        as fn(
            &ConnectionService,
            AgentId,
            &str,
            &str,
            &str,
        ) -> crate::error::Result<(Account, crate::services::ActiveBinding)>;
    let _ = ConnectionService::update_provider_non_current
        as fn(&ConnectionService, &Provider) -> crate::error::Result<Provider>;
    let _ = svc.clear(AgentId::Claude); // smoke: clear is the dual-write path
}

#[test]
fn model_profile_only_binding_survives_get_active() {
    let (_d, db) = tmp();
    let conn = ConnectionService::new(db.clone());
    seed_extension_fields(&db, "claude", Some("model-1"), Some("profile-1"));

    let active = conn.get_active(AgentId::Claude).unwrap().unwrap();
    assert!(active.account_id.is_none());
    assert!(active.provider_id.is_none());
    assert_eq!(active.model_id.as_deref(), Some("model-1"));
    assert_eq!(active.config_profile_id.as_deref(), Some("profile-1"));
}

#[test]
fn model_profile_only_binding_clears_stale_legacy_currents() {
    let (_d, db) = tmp();
    let accounts = AccountRepo::new(db.clone());
    let providers = ProviderRepo::new(db.clone());
    let conn = ConnectionService::new(db.clone());

    accounts
        .create(&account("stale-acc", AgentId::Claude, true, "t1"))
        .unwrap();
    providers
        .create(&provider("stale-prov", AgentId::Claude, false, "t1"))
        .unwrap();
    db.with_conn(|c| {
        c.execute(
            "UPDATE providers SET is_current = 1 WHERE id = 'stale-prov'",
            [],
        )?;
        Ok(())
    })
    .unwrap();
    seed_extension_fields(&db, "claude", Some("m"), Some("p"));

    let active = conn.get_active(AgentId::Claude).unwrap().unwrap();
    assert_eq!(active.model_id.as_deref(), Some("m"));
    assert_eq!(active.config_profile_id.as_deref(), Some("p"));
    assert!(active.account_id.is_none());
    assert!(accounts.get_current(AgentId::Claude).unwrap().is_none());
    assert!(providers.get_current(AgentId::Claude).unwrap().is_none());
}

#[test]
fn activate_account_preserves_existing_model_and_profile() {
    let (_d, db) = tmp();
    let accounts = AccountRepo::new(db.clone());
    let conn = ConnectionService::new(db.clone());
    seed_extension_fields(&db, "claude", Some("keep-model"), Some("keep-profile"));
    accounts
        .create(&account("acc1", AgentId::Claude, false, "t1"))
        .unwrap();

    let (_acc, b) = conn
        .activate_account(AgentId::Claude, "acc1", "t1", "2026-06-01 12:00:00.000000")
        .unwrap();
    assert_eq!(b.account_id.as_deref(), Some("acc1"));
    assert_eq!(b.model_id.as_deref(), Some("keep-model"));
    assert_eq!(b.config_profile_id.as_deref(), Some("keep-profile"));
}

#[test]
fn activate_provider_preserves_existing_model_and_profile() {
    let (_d, db) = tmp();
    let providers = ProviderRepo::new(db.clone());
    let conn = ConnectionService::new(db.clone());
    seed_extension_fields(&db, "codex", Some("m2"), Some("p2"));
    providers
        .create(&provider("prov1", AgentId::Codex, false, "t1"))
        .unwrap();

    let (_p, b) = conn
        .activate_provider(AgentId::Codex, "prov1", "t1", "2026-06-01 12:00:00.000000")
        .unwrap();
    assert_eq!(b.provider_id.as_deref(), Some("prov1"));
    assert_eq!(b.model_id.as_deref(), Some("m2"));
    assert_eq!(b.config_profile_id.as_deref(), Some("p2"));
}

#[test]
fn demote_provider_clears_connection_but_keeps_model_profile() {
    let (_d, db) = tmp();
    let providers = ProviderService::new(db.clone());
    let conn = ConnectionService::new(db.clone());
    providers
        .create(&provider_input("p-cur", AgentId::Claude, true))
        .unwrap();
    seed_extension_fields(&db, "claude", Some("model-x"), Some("profile-x"));

    providers
        .update(&provider_input("p-cur", AgentId::Claude, false))
        .unwrap();

    let active = conn.get_active(AgentId::Claude).unwrap().unwrap();
    assert!(active.account_id.is_none());
    assert!(active.provider_id.is_none());
    assert_eq!(active.model_id.as_deref(), Some("model-x"));
    assert_eq!(active.config_profile_id.as_deref(), Some("profile-x"));
    assert!(ProviderRepo::new(db)
        .get_current(AgentId::Claude)
        .unwrap()
        .is_none());
}

#[test]
fn delete_active_account_keeps_model_profile() {
    let (_d, db) = tmp();
    let accounts = AccountService::new(db.clone());
    let conn = ConnectionService::new(db.clone());
    let created = accounts
        .create(crate::models::AccountInput {
            agent_id: AgentId::Claude,
            kind: AccountKind::ApiKey,
            label: "to-delete".into(),
            credentials: json!({ "k": 1 }),
            extra: json!({}),
            is_current: true,
        })
        .unwrap();
    seed_extension_fields(&db, "claude", Some("m-del"), Some("p-del"));

    accounts.delete(&created.id, AgentId::Claude).unwrap();
    let active = conn.get_active(AgentId::Claude).unwrap().unwrap();
    assert!(active.account_id.is_none());
    assert_eq!(active.model_id.as_deref(), Some("m-del"));
    assert_eq!(active.config_profile_id.as_deref(), Some("p-del"));
}

#[test]
fn dangling_connection_repair_preserves_model_profile() {
    let (_d, db) = tmp();
    let conn = ConnectionService::new(db.clone());
    let bindings = ActiveBindingRepo::new(db.clone());
    bindings
        .upsert(&ActiveBindingRow {
            agent_key: "claude".into(),
            account_id: Some("ghost".into()),
            provider_id: None,
            model_id: Some("m-ghost".into()),
            config_profile_id: Some("p-ghost".into()),
            revision: 1,
            created_at: "t0".into(),
            updated_at: "t0".into(),
        })
        .unwrap();

    let active = conn.get_active(AgentId::Claude).unwrap().unwrap();
    assert!(active.account_id.is_none());
    assert!(active.provider_id.is_none());
    assert_eq!(active.model_id.as_deref(), Some("m-ghost"));
    assert_eq!(active.config_profile_id.as_deref(), Some("p-ghost"));
}

#[test]
fn clear_deletes_entire_binding_including_model_profile() {
    let (_d, db) = tmp();
    let accounts = AccountRepo::new(db.clone());
    let conn = ConnectionService::new(db.clone());
    accounts
        .create(&account("a", AgentId::Kimi, false, "t1"))
        .unwrap();
    conn.activate_account(AgentId::Kimi, "a", "t1", "t2")
        .unwrap();
    seed_extension_fields(&db, "kimi", Some("m"), Some("p"));
    assert!(conn
        .get_active(AgentId::Kimi)
        .unwrap()
        .unwrap()
        .model_id
        .is_some());

    conn.clear(AgentId::Kimi).unwrap();
    assert!(conn.get_active(AgentId::Kimi).unwrap().is_none());
    assert!(ActiveBindingRepo::new(db).get("kimi").unwrap().is_none());
}

#[test]
fn fully_empty_binding_row_returns_none() {
    let (_d, db) = tmp();
    let conn = ConnectionService::new(db.clone());
    ActiveBindingRepo::new(db.clone())
        .upsert(&ActiveBindingRow {
            agent_key: "grok".into(),
            account_id: None,
            provider_id: None,
            model_id: None,
            config_profile_id: None,
            revision: 1,
            created_at: "t0".into(),
            updated_at: "t0".into(),
        })
        .unwrap();

    assert!(conn.get_active(AgentId::Grok).unwrap().is_none());
    assert!(ActiveBindingRepo::new(db).get("grok").unwrap().is_none());
}

#[test]
fn delete_account_moves_to_trash_and_restore_does_not_reactivate() {
    let (_d, db) = tmp();
    let accounts = AccountRepo::new(db.clone());
    let conn = ConnectionService::new(db.clone());
    let created = accounts
        .create(&account("trash-account", AgentId::Claude, true, "t1"))
        .unwrap();

    conn.delete_account(&created.id, AgentId::Claude).unwrap();
    assert!(accounts.get_by_id(&created.id).unwrap().is_none());
    let trash = conn.list_trash(Some(AgentId::Claude)).unwrap();
    assert_eq!(trash.len(), 1);
    assert_eq!(trash[0].source_id, created.id);
    assert!(trash[0].account.is_some());
    assert!(trash[0].account.as_ref().unwrap().credentials.is_object());

    let trash_id = trash[0].id.clone();
    conn.restore_trash(&trash_id).unwrap();
    let restored = accounts.get_by_id(&created.id).unwrap().unwrap();
    assert!(!restored.is_current);
    assert!(conn.list_trash(Some(AgentId::Claude)).unwrap().is_empty());
}

fn grok_mixed_account(id: &str, current: bool) -> Account {
    Account {
        id: id.into(),
        agent_id: AgentId::Grok,
        kind: AccountKind::ApiKey,
        label: "API Key".into(),
        credentials: json!({
            "format": "grok_bundle",
            "api_key": "xai-file-key-12345678",
            "content": "[model.\"grok\"]\napi_key = \"xai-file-key-12345678\"\n",
            "auth": {
                "https://auth.x.ai::b1a00492-073a-47ea-816f-4c329264a828": {
                    "email": "a@example.com",
                    "refresh_token": "rt-oauth-restore",
                    "access_token": "at-oauth-restore"
                }
            }
        }),
        extra: json!({ "source": "config.toml+auth.json" }),
        status: "active".into(),
        is_current: current,
        created_at: "t1".into(),
        updated_at: "t1".into(),
    }
}

#[test]
fn restore_mixed_grok_trash_splits_oauth_and_api_key() {
    let (_d, db) = tmp();
    let accounts = AccountRepo::new(db.clone());
    let conn = ConnectionService::new(db.clone());
    let created = accounts
        .create(&grok_mixed_account("grok-mixed", true))
        .unwrap();

    conn.delete_account(&created.id, AgentId::Grok).unwrap();
    let trash = conn.list_trash(Some(AgentId::Grok)).unwrap();
    conn.restore_trash(&trash[0].id).unwrap();

    let restored = accounts.list(Some(AgentId::Grok)).unwrap();
    assert_eq!(restored.len(), 2);
    assert!(restored.iter().all(|row| !row.is_current));
    let oauth = restored
        .iter()
        .find(|row| row.kind == AccountKind::Oauth)
        .expect("oauth");
    let key = restored
        .iter()
        .find(|row| row.kind == AccountKind::ApiKey)
        .expect("api key");
    assert_eq!(oauth.id, created.id);
    assert_eq!(oauth.credentials["format"], "auth_json");
    assert!(!oauth
        .credentials
        .to_string()
        .contains("xai-file-key-12345678"));
    assert_eq!(key.credentials["format"], "api_key");
    assert_eq!(key.credentials["api_key"], "xai-file-key-12345678");
    assert!(conn.list_trash(Some(AgentId::Grok)).unwrap().is_empty());
}

#[test]
fn restore_mixed_grok_trash_skips_family_already_in_pool() {
    let (_d, db) = tmp();
    let accounts = AccountRepo::new(db.clone());
    let conn = ConnectionService::new(db.clone());
    accounts
        .create(&Account {
            id: "grok-oauth-existing".into(),
            agent_id: AgentId::Grok,
            kind: AccountKind::Oauth,
            label: "a@example.com".into(),
            credentials: json!({
                "format": "auth_json",
                "refresh_token": "rt-oauth-restore",
                "access_token": "at-oauth-restore",
                "body": {
                    "https://auth.x.ai::b1a00492-073a-47ea-816f-4c329264a828": {
                        "refresh_token": "rt-oauth-restore"
                    }
                }
            }),
            extra: json!({}),
            status: "active".into(),
            is_current: false,
            created_at: "t0".into(),
            updated_at: "t0".into(),
        })
        .unwrap();
    let created = accounts
        .create(&grok_mixed_account("grok-mixed-2", false))
        .unwrap();
    conn.delete_account(&created.id, AgentId::Grok).unwrap();
    let trash = conn.list_trash(Some(AgentId::Grok)).unwrap();
    conn.restore_trash(&trash[0].id).unwrap();

    let restored = accounts.list(Some(AgentId::Grok)).unwrap();
    let oauth: Vec<_> = restored
        .iter()
        .filter(|row| row.kind == AccountKind::Oauth)
        .collect();
    let keys: Vec<_> = restored
        .iter()
        .filter(|row| row.kind == AccountKind::ApiKey)
        .collect();
    assert_eq!(oauth.len(), 1);
    assert_eq!(oauth[0].id, "grok-oauth-existing");
    assert_eq!(keys.len(), 1);
    assert_eq!(keys[0].credentials["api_key"], "xai-file-key-12345678");
}

#[test]
fn trash_keeps_file_snapshot_but_list_redacts_content() {
    let (_d, db) = tmp();
    let accounts = AccountRepo::new(db.clone());
    let conn = ConnectionService::new(db.clone());
    let secret = "xai-secret-value-here";
    let created = accounts
        .create(&Account {
            id: "grok-snap".into(),
            agent_id: AgentId::Grok,
            kind: AccountKind::ApiKey,
            label: "API Key".into(),
            credentials: json!({
                "format": "api_key",
                "api_key": secret,
                "content": format!("[model.\"grok\"]\napi_key = \"{secret}\"\n"),
            }),
            extra: json!({}),
            status: "active".into(),
            is_current: false,
            created_at: "t1".into(),
            updated_at: "t1".into(),
        })
        .unwrap();

    conn.delete_account(&created.id, AgentId::Grok).unwrap();
    let trash = conn.list_trash(Some(AgentId::Grok)).unwrap();
    let stored = trash[0].account.as_ref().unwrap();
    assert!(
        stored.credentials["content"]
            .as_str()
            .unwrap_or("")
            .contains(secret),
        "recovery payload must keep the file snapshot"
    );

    let listed = trash[0].redacted();
    let listed_content = listed.account.as_ref().unwrap().credentials["content"]
        .as_str()
        .unwrap_or("");
    assert!(!listed_content.contains(secret), "{listed_content}");
    assert_eq!(
        listed.account.as_ref().unwrap().credentials["api_key"],
        "***"
    );

    conn.restore_trash(&trash[0].id).unwrap();
    let restored = accounts.get_by_id(&created.id).unwrap().unwrap();
    assert!(restored.credentials["content"]
        .as_str()
        .unwrap_or("")
        .contains(secret));
    assert!(!restored.is_current);
}

#[test]
fn list_trash_recovers_last4_from_old_unnamed_grok_payload() {
    let (_d, db) = tmp();
    let conn = ConnectionService::new(db.clone());
    let old = Account {
        id: "grok-acc-old".into(),
        agent_id: AgentId::Grok,
        kind: AccountKind::ApiKey,
        label: "API Key".into(),
        credentials: json!({ "format": "api_key", "api_key": "***" }),
        extra: json!({ "identityLabel": "xai-••••8660 (API Key)" }),
        status: "active".into(),
        is_current: true,
        created_at: "2026-08-28 02:19:53.000000".into(),
        updated_at: "2026-08-28 02:19:53.000000".into(),
    };
    db.with_conn(|c| {
        ConnectionTrashRepo::insert_conn(
            c,
            &old.id,
            AgentId::Grok,
            ConnectionTrashKind::Account,
            "API Key",
            true,
            &old,
            "2026-08-28 02:19:53.000000",
        )
    })
    .unwrap();

    let listed = conn.list_trash(Some(AgentId::Grok)).unwrap();
    assert_eq!(listed.len(), 1);
    let account = listed[0].account.as_ref().expect("account");
    assert_eq!(account.extra["secretTail"], "**8660");
    assert_eq!(account.redacted().extra["secretTail"], "**8660");

    let persisted = conn.list_trash(Some(AgentId::Grok)).unwrap();
    assert_eq!(
        persisted[0].account.as_ref().unwrap().extra["secretTail"],
        "**8660"
    );
}

#[test]
fn list_trash_recovers_last4_from_unredacted_old_credentials() {
    let (_d, db) = tmp();
    let conn = ConnectionService::new(db.clone());
    let old = Account {
        id: "grok-acc-key".into(),
        agent_id: AgentId::Grok,
        kind: AccountKind::ApiKey,
        label: "API Key".into(),
        credentials: json!({
            "format": "api_key",
            "api_key": "xai-secret-key-value-8660"
        }),
        extra: json!({}),
        status: "active".into(),
        is_current: false,
        created_at: "2026-08-28 01:20:16.000000".into(),
        updated_at: "2026-08-28 01:20:16.000000".into(),
    };
    db.with_conn(|c| {
        ConnectionTrashRepo::insert_conn(
            c,
            &old.id,
            AgentId::Grok,
            ConnectionTrashKind::Account,
            "API Key",
            false,
            &old,
            "2026-08-28 01:20:16.000000",
        )
    })
    .unwrap();

    let listed = conn.list_trash(Some(AgentId::Grok)).unwrap();
    assert_eq!(
        listed[0].account.as_ref().unwrap().extra["secretTail"],
        "**8660"
    );
    let redacted = listed[0].redacted();
    assert_eq!(
        redacted.account.as_ref().unwrap().credentials["api_key"],
        "***"
    );
    assert_eq!(
        redacted.account.as_ref().unwrap().extra["secretTail"],
        "**8660"
    );
    assert!(!serde_json::to_string(&redacted)
        .unwrap()
        .contains("xai-secret-key-value-8660"));
}

#[test]
fn list_trash_recovers_last4_from_other_stored_mask_field() {
    let (_d, db) = tmp();
    let conn = ConnectionService::new(db.clone());
    let old = Account {
        id: "grok-acc-preview".into(),
        agent_id: AgentId::Grok,
        kind: AccountKind::ApiKey,
        label: "API Key".into(),
        credentials: json!({ "format": "api_key", "api_key": "***" }),
        extra: json!({
            "identityLabel": "API Key",
            "preview": "xai-••••272f (API Key)"
        }),
        status: "active".into(),
        is_current: false,
        created_at: "2026-08-28 01:20:16.000000".into(),
        updated_at: "2026-08-28 01:20:16.000000".into(),
    };
    db.with_conn(|c| {
        ConnectionTrashRepo::insert_conn(
            c,
            &old.id,
            AgentId::Grok,
            ConnectionTrashKind::Account,
            "API Key",
            false,
            &old,
            "2026-08-28 01:20:16.000000",
        )
    })
    .unwrap();

    let listed = conn.list_trash(Some(AgentId::Grok)).unwrap();
    assert_eq!(
        listed[0].account.as_ref().unwrap().extra["secretTail"],
        "**272f"
    );
}

#[test]
fn delete_current_grok_api_key_persists_stored_last4_and_host() {
    let (_d, db) = tmp();
    let accounts = AccountRepo::new(db.clone());
    let conn = ConnectionService::new(db.clone());
    let created = accounts
        .create(&Account {
            id: "grok-live-keep".into(),
            agent_id: AgentId::Grok,
            kind: AccountKind::ApiKey,
            label: "API Key".into(),
            credentials: json!({ "format": "api_key", "api_key": "***" }),
            extra: json!({
                "secretTail": "**8660",
                "endpoint": "https://api.x.ai/v1"
            }),
            status: "active".into(),
            is_current: true,
            created_at: "2026-08-28 02:19:53.000000".into(),
            updated_at: "2026-08-28 02:19:53.000000".into(),
        })
        .unwrap();

    conn.delete_account(&created.id, AgentId::Grok).unwrap();
    let trash = conn.list_trash(Some(AgentId::Grok)).unwrap();
    assert_eq!(trash.len(), 1);
    let account = trash[0].account.as_ref().expect("account");
    assert_eq!(account.extra["secretTail"], "**8660");
    assert_eq!(account.extra["endpoint"], "https://api.x.ai/v1");
    assert_eq!(account.label, "API Key");
}

#[test]
fn delete_current_grok_api_key_persists_live_last4_and_host() {
    static GROK_HOME_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = GROK_HOME_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let live = tempfile::tempdir().unwrap();
    let prev = std::env::var_os("GROK_HOME");
    std::env::set_var("GROK_HOME", live.path());
    std::fs::write(
        live.path().join("config.toml"),
        r#"[models]
default = "grok"

[model."grok"]
model = "grok-4.6"
base_url = "https://api.x.ai/v1"
api_key = "xai-secret-key-value-8660"
api_backend = "responses"
"#,
    )
    .unwrap();

    let (_d, db) = tmp();
    let accounts = AccountRepo::new(db.clone());
    let conn = ConnectionService::new(db.clone());
    let created = accounts
        .create(&Account {
            id: "grok-live-from-file".into(),
            agent_id: AgentId::Grok,
            kind: AccountKind::ApiKey,
            label: "API Key".into(),
            credentials: json!({ "format": "api_key", "api_key": "***" }),
            extra: json!({}),
            status: "active".into(),
            is_current: true,
            created_at: "2026-08-28 02:19:53.000000".into(),
            updated_at: "2026-08-28 02:19:53.000000".into(),
        })
        .unwrap();

    conn.delete_account(&created.id, AgentId::Grok).unwrap();
    match prev {
        Some(value) => std::env::set_var("GROK_HOME", value),
        None => std::env::remove_var("GROK_HOME"),
    }
    let trash = conn.list_trash(Some(AgentId::Grok)).unwrap();
    assert_eq!(trash.len(), 1);
    let account = trash[0].account.as_ref().expect("account");
    assert_eq!(account.extra["secretTail"], "**8660");
    assert_eq!(account.extra["endpoint"], "https://api.x.ai/v1");
}

#[test]
fn list_trash_does_not_invent_last4_when_payload_has_no_identity() {
    let (_d, db) = tmp();
    let conn = ConnectionService::new(db.clone());
    let old = Account {
        id: "grok-acc-ghost".into(),
        agent_id: AgentId::Grok,
        kind: AccountKind::ApiKey,
        label: "API Key".into(),
        credentials: json!({ "format": "grok_bundle", "api_key": "***" }),
        extra: json!({ "source": "live", "identityLabel": "API Key" }),
        status: "active".into(),
        is_current: false,
        created_at: "2026-08-28 01:20:16.000000".into(),
        updated_at: "2026-08-28 01:20:16.000000".into(),
    };
    db.with_conn(|c| {
        ConnectionTrashRepo::insert_conn(
            c,
            &old.id,
            AgentId::Grok,
            ConnectionTrashKind::Account,
            "API Key",
            false,
            &old,
            "2026-08-28 01:20:16.000000",
        )
    })
    .unwrap();

    let listed = conn.list_trash(Some(AgentId::Grok)).unwrap();
    let extra = &listed[0].account.as_ref().unwrap().extra;
    assert!(extra.get("secretTail").is_none(), "{extra}");
    assert_eq!(
        listed[0]
            .redacted()
            .account
            .as_ref()
            .unwrap()
            .extra
            .get("secretTail"),
        None
    );
}

#[test]
fn list_trash_recovers_provider_last4_from_stored_name() {
    let (_d, db) = tmp();
    let conn = ConnectionService::new(db.clone());
    let old = Provider {
        id: "grok-prov-old".into(),
        agent_id: AgentId::Grok,
        name: "xai-••••6aa9 (API Key)".into(),
        settings_config: json!({
            "format": "toml",
            "content": "[model.\"grok\"]\napi_key = \"***\"\n"
        }),
        meta: json!({}),
        is_current: false,
        created_at: "2026-08-28 02:19:53.000000".into(),
        updated_at: "2026-08-28 02:19:53.000000".into(),
    };
    db.with_conn(|c| {
        ConnectionTrashRepo::insert_conn(
            c,
            &old.id,
            AgentId::Grok,
            ConnectionTrashKind::Provider,
            "API Key",
            false,
            &old,
            "2026-08-28 02:19:53.000000",
        )
    })
    .unwrap();

    let listed = conn.list_trash(Some(AgentId::Grok)).unwrap();
    assert_eq!(
        listed[0].provider.as_ref().unwrap().meta["secretTail"],
        "**6aa9"
    );
}

#[test]
fn provider_trash_can_be_permanently_deleted() {
    let (_d, db) = tmp();
    let providers = ProviderRepo::new(db.clone());
    let conn = ConnectionService::new(db.clone());
    let created = providers
        .create(&provider("trash-provider", AgentId::Codex, false, "t1"))
        .unwrap();

    conn.delete_provider(&created.id, AgentId::Codex).unwrap();
    let trash = conn.list_trash(Some(AgentId::Codex)).unwrap();
    assert_eq!(trash.len(), 1);
    assert_eq!(trash[0].provider.as_ref().unwrap().id, created.id);
    conn.delete_trash(&trash[0].id).unwrap();
    assert!(conn.list_trash(Some(AgentId::Codex)).unwrap().is_empty());
}

/// D3: official activate dual-writes is_current + agent_active_bindings.
/// AccountRepo.update flips only is_current (no pointer write).
#[test]
fn ticket_connection_pointer_stays_when_account_repo_flips_is_current() {
    let (_d, db) = tmp();
    let accounts = AccountRepo::new(db.clone());
    accounts
        .create(&account("acc-a", AgentId::Claude, false, "t1"))
        .unwrap();
    accounts
        .create(&account("acc-b", AgentId::Claude, false, "t2"))
        .unwrap();
    let conn = ConnectionService::new(db.clone());
    conn.record_account_active(AgentId::Claude, "acc-a")
        .unwrap();
    let pointer = ActiveBindingRepo::new(db.clone())
        .get("claude")
        .unwrap()
        .unwrap();
    assert_eq!(pointer.account_id.as_deref(), Some("acc-a"));
    assert!(accounts.get_by_id("acc-a").unwrap().unwrap().is_current);

    let mut b = accounts.get_by_id("acc-b").unwrap().unwrap();
    b.is_current = true;
    accounts.update(&b).unwrap();
    assert!(accounts.get_by_id("acc-b").unwrap().unwrap().is_current);
    assert!(!accounts.get_by_id("acc-a").unwrap().unwrap().is_current);
    let drifted = ActiveBindingRepo::new(db).get("claude").unwrap().unwrap();
    assert_eq!(drifted.account_id.as_deref(), Some("acc-a"));
}

#[test]
fn list_trash_filtered_keeps_connections_and_pool_homes_apart() {
    let (_d, db) = tmp();
    let accounts = AccountRepo::new(db.clone());
    let conn = ConnectionService::new(db.clone());
    let mut pool_owned = account("pool-acc", AgentId::Claude, false, "t1");
    pool_owned.extra = json!({ "home": "route_pool" });
    accounts
        .create(&account("conn-acc", AgentId::Claude, false, "t1"))
        .unwrap();
    accounts.create(&pool_owned).unwrap();

    conn.delete_account("conn-acc", AgentId::Claude).unwrap();
    conn.delete_account("pool-acc", AgentId::Claude).unwrap();

    let connections = conn
        .list_trash_filtered(Some(AgentId::Claude), Some("connections"))
        .unwrap();
    let pool = conn
        .list_trash_filtered(Some(AgentId::Claude), Some("route_pool"))
        .unwrap();
    assert_eq!(connections.len(), 1);
    assert_eq!(connections[0].source_id, "conn-acc");
    assert_eq!(connections[0].home, "connections");
    assert_eq!(pool.len(), 1);
    assert_eq!(pool[0].source_id, "pool-acc");
    assert_eq!(pool[0].home, "route_pool");
}
