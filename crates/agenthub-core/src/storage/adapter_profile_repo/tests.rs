use crate::models::{
    AdapterProfile, AdapterProfileFilter, AdapterProfileStatus, AdapterRoute, AdapterSourceKind,
    AgentId,
};
use crate::storage::{AdapterProfileRepo, Database};

fn tmp_db() -> (tempfile::TempDir, Database) {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("adapter-profile.db")).unwrap();
    (dir, db)
}

fn sample_profile(id: &str, name: &str) -> AdapterProfile {
    AdapterProfile {
        id: id.into(),
        name: name.into(),
        source_kind: AdapterSourceKind::Account,
        source_id: "account-1".into(),
        target_agent_id: AgentId::Codex,
        route: AdapterRoute::ConfigSync,
        status: AdapterProfileStatus::Applying,
        rule_id: "account-to-codex".into(),
        rule_version: "v1".into(),
        generated_provider_id: None,
        local_port: None,
        auto_start: false,
        last_error_code: None,
        created_at: "t0".into(),
        updated_at: "t0".into(),
    }
}

#[test]
fn migration_schema_is_credential_free_and_has_required_indexes() {
    let (_dir, db) = tmp_db();
    db.with_conn(|conn| {
        let columns = conn
            .prepare("PRAGMA table_info(adapter_profiles)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        assert_eq!(columns.len(), 15);
        assert!(columns.iter().any(|column| column == "local_port"));
        assert!(columns.iter().any(|column| column == "auto_start"));
        for forbidden in ["credentials", "api_key", "secret", "config"] {
            assert!(
                !columns.iter().any(|column| column.eq_ignore_ascii_case(forbidden)),
                "schema must not persist {forbidden}",
            );
        }

        let indexes = conn
            .prepare("PRAGMA index_list(adapter_profiles)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        assert!(indexes.iter().any(|name| name == "idx_adapter_profiles_source_target"));
        assert!(indexes.iter().any(|name| name == "idx_adapter_profiles_generated_provider"));
        assert!(indexes.iter().any(|name| name == "idx_adapter_profiles_bridge_restore"));
        let version: i64 = conn.query_row(
            "SELECT COUNT(*) FROM schema_migrations WHERE version = '00012_adapter_profiles'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(version, 1);
        let bridge_version: i64 = conn.query_row(
            "SELECT COUNT(*) FROM schema_migrations WHERE version = '00013_adapter_bridge_profiles'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(bridge_version, 1);
        Ok(())
    })
    .unwrap();
}

#[test]
fn create_get_update_and_delete_preserve_creation_time() {
    let (_dir, db) = tmp_db();
    let repo = AdapterProfileRepo::new(db);
    let created = repo
        .create(&sample_profile("profile-1", "Original"))
        .unwrap();
    assert_eq!(repo.get("profile-1").unwrap(), Some(created.clone()));

    let mut update = created.clone();
    update.name = "Renamed".into();
    update.status = AdapterProfileStatus::Active;
    update.generated_provider_id = Some("provider-created-during-apply".into());
    update.route = AdapterRoute::LocalBridge;
    update.local_port = Some(43121);
    update.auto_start = false;
    update.updated_at = "t1".into();
    update.created_at = "caller-must-not-replace-this".into();
    let saved = repo.update(&update).unwrap();
    assert_eq!(saved.name, "Renamed");
    assert_eq!(saved.status, AdapterProfileStatus::Active);
    assert_eq!(saved.created_at, "t0");
    assert_eq!(saved.updated_at, "t1");
    assert_eq!(saved.local_port, Some(43121));
    assert!(!saved.auto_start);
    assert_eq!(
        saved.generated_provider_id.as_deref(),
        Some("provider-created-during-apply")
    );

    repo.delete("profile-1").unwrap();
    assert_eq!(repo.get("profile-1").unwrap(), None);
    assert_eq!(repo.delete("profile-1").unwrap_err().code(), "not_found");
}

#[test]
fn create_is_idempotent_and_list_is_stable_by_source_and_target() {
    let (_dir, db) = tmp_db();
    let repo = AdapterProfileRepo::new(db);
    let first = repo.create(&sample_profile("profile-1", "Zulu")).unwrap();
    let mut duplicate = sample_profile("profile-1", "Ignored on retry");
    duplicate.updated_at = "t9".into();
    assert_eq!(repo.create_or_get(&duplicate).unwrap(), first);

    repo.create(&sample_profile("profile-2", "Alpha")).unwrap();
    let mut other_target = sample_profile("profile-3", "Beta");
    other_target.target_agent_id = AgentId::Claude;
    repo.create(&other_target).unwrap();
    let mut other_source = sample_profile("profile-4", "Gamma");
    other_source.source_id = "account-2".into();
    repo.create(&other_source).unwrap();

    let listed = repo
        .list(
            Some(AdapterSourceKind::Account),
            Some("account-1"),
            Some(AgentId::Codex),
        )
        .unwrap();
    assert_eq!(
        listed
            .iter()
            .map(|profile| profile.id.as_str())
            .collect::<Vec<_>>(),
        vec!["profile-2", "profile-1"],
    );
}

#[test]
fn unsupported_route_and_invalid_database_enums_fail_closed() {
    let (_dir, db) = tmp_db();
    let repo = AdapterProfileRepo::new(db.clone());
    let mut unsupported = sample_profile("unsupported", "Unsupported");
    unsupported.route = AdapterRoute::Unsupported;
    assert_eq!(repo.create(&unsupported).unwrap_err().code(), "invalid_arg");

    let valid = sample_profile("profile-1", "Valid");
    repo.create(&valid).unwrap();
    db.with_conn(|conn| {
        assert!(conn
            .execute(
                "INSERT INTO adapter_profiles (id, name, source_kind, source_id, target_agent_id, route, status, rule_id, rule_version, created_at, updated_at) VALUES ('bad', 'Bad', 'account', 'a', 'codex', 'config_sync', 'not-a-status', 'r', 'v', 't0', 't0')",
                [],
            )
            .is_err());
        conn.execute_batch("PRAGMA ignore_check_constraints = ON")?;
        conn.execute(
            "UPDATE adapter_profiles SET status = 'not-a-status' WHERE id = ?1",
            ["profile-1"],
        )?;
        conn.execute_batch("PRAGMA ignore_check_constraints = OFF")?;
        Ok(())
    })
    .unwrap();
    assert_eq!(
        repo.get("profile-1").unwrap_err().code(),
        "adapter_profile.invalid_data"
    );
}

#[test]
fn bridge_fields_filter_and_invalid_persisted_values_fail_closed() {
    let (_dir, db) = tmp_db();
    let repo = AdapterProfileRepo::new(db.clone());
    let mut bridge = sample_profile("bridge", "Bridge");
    bridge.route = AdapterRoute::LocalBridge;
    bridge.local_port = Some(43121);
    bridge.auto_start = true;
    bridge.status = AdapterProfileStatus::Active;
    repo.create(&bridge).unwrap();
    let mut disabled = sample_profile("disabled", "Disabled");
    disabled.route = AdapterRoute::LocalBridge;
    disabled.auto_start = false;
    repo.create(&disabled).unwrap();

    let filtered = repo
        .list_filtered(&AdapterProfileFilter {
            route: Some(AdapterRoute::LocalBridge),
            status: Some(AdapterProfileStatus::Active),
            auto_start: Some(true),
            ..AdapterProfileFilter::default()
        })
        .unwrap();
    assert_eq!(filtered, vec![bridge.clone()]);

    db.with_conn(|conn| {
        conn.execute_batch("PRAGMA ignore_check_constraints = ON")?;
        conn.execute(
            "UPDATE adapter_profiles SET local_port = 0 WHERE id = ?1",
            ["bridge"],
        )?;
        conn.execute_batch("PRAGMA ignore_check_constraints = OFF")?;
        Ok(())
    })
    .unwrap();
    assert_eq!(
        repo.get("bridge").unwrap_err().code(),
        "adapter_profile.invalid_data"
    );

    db.with_conn(|conn| {
        conn.execute_batch("PRAGMA ignore_check_constraints = ON")?;
        conn.execute(
            "UPDATE adapter_profiles SET local_port = 43121, auto_start = 7 WHERE id = ?1",
            ["bridge"],
        )?;
        conn.execute_batch("PRAGMA ignore_check_constraints = OFF")?;
        Ok(())
    })
    .unwrap();
    assert_eq!(
        repo.get("bridge").unwrap_err().code(),
        "adapter_profile.invalid_data"
    );
}

#[test]
fn migration_defaults_existing_native_profile_to_no_auto_start_and_rejects_new_native_auto_start() {
    let (_dir, db) = tmp_db();
    db.with_conn(|conn| {
        // This mirrors a profile created by 00012 before 00013 added the
        // runtime fields: SQLite supplies the migration default on read.
        conn.execute(
            "INSERT INTO adapter_profiles (id, name, source_kind, source_id, target_agent_id, route, status, rule_id, rule_version, created_at, updated_at) VALUES ('native-before-00013', 'Native', 'provider', 'source', 'claude', 'native_endpoint', 'active', 'r', '1', 't0', 't0')",
            [],
        )?;
        Ok(())
    })
    .unwrap();
    let repo = AdapterProfileRepo::new(db);
    let migrated = repo.get("native-before-00013").unwrap().unwrap();
    assert_eq!(migrated.local_port, None);
    assert!(!migrated.auto_start);

    let mut invalid = sample_profile("native-auto-start", "Invalid native auto start");
    invalid.auto_start = true;
    assert_eq!(repo.create(&invalid).unwrap_err().code(), "invalid_arg");
}

#[test]
fn profile_serialization_cannot_carry_credential_or_config_fields() {
    let value = serde_json::to_value(sample_profile("profile-1", "No payload")).unwrap();
    for forbidden in ["credentials", "api_key", "secret", "config"] {
        assert!(
            value.get(forbidden).is_none(),
            "profile payload has forbidden {forbidden} field"
        );
    }
    assert_eq!(value["sourceKind"], "account");
    assert_eq!(value["targetAgentId"], "codex");
    assert_eq!(value["route"], "config_sync");
    assert!(value.get("generatedProviderId").is_none());
    assert!(value.get("localPort").is_none());
    assert_eq!(value["autoStart"], false);
    assert!(value.get("lastErrorCode").is_none());
}
