use super::*;

use crate::models::Provider;
use crate::services::ProviderService;
use crate::storage::{AdapterProfileRepo, ProviderRepo};
use axum::http::StatusCode;
use axum::routing::get;
use axum::Router;
use tokio::net::TcpListener;

async fn health_upstream(status: StatusCode) -> (u16, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    let app = Router::new().route("/models", get(move || async move { status }));
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (port, task)
}

fn test_db() -> (tempfile::TempDir, Database) {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("adapter-bridge.db")).unwrap();
    (dir, db)
}

fn kimi_source(id: &str, api_key: &str) -> Provider {
    Provider {
        id: id.into(),
        agent_id: AgentId::Kimi,
        name: "Kimi membership".into(),
        settings_config: json!({"apiKey": api_key}),
        meta: json!({"preset": "kimi-code-membership"}),
        is_current: false,
        created_at: "now".into(),
        updated_at: "now".into(),
    }
}

fn request(source_id: &str) -> AdapterBridgePrepareRequest {
    AdapterBridgePrepareRequest {
        source_kind: AdapterSourceKind::Provider,
        source_id: source_id.into(),
        target_agent_id: AgentId::Codex,
        auto_start: true,
    }
}

fn create_projection(db: &Database, prepared: &AdapterBridgePrepared, port: u16) -> Provider {
    let input = match prepared.provider_projection(port).unwrap() {
        AdapterBridgeProviderProjection::Create(input) => input,
        other => panic!("expected create projection, got {other:?}"),
    };
    ProviderService::new(db.clone()).create(&input).unwrap()
}

#[test]
fn prepare_project_finalize_and_restore_keep_source_secret_out_of_persistence() {
    let (_dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&kimi_source(
            "kimi-membership",
            "upstream-membership-secret",
        ))
        .unwrap();
    let service = AdapterBridgeService::new(db.clone());

    let prepared = service.prepare(&request("kimi-membership")).unwrap();
    assert_eq!(prepared.profile().status, AdapterProfileStatus::Applying);
    assert_eq!(prepared.profile().route, AdapterRoute::LocalBridge);
    assert!(prepared.profile().auto_start);
    assert_eq!(prepared.profile().local_port, None);
    assert!(prepared.profile().generated_provider_id.is_some());
    assert!(!format!("{prepared:?}").contains("upstream-membership-secret"));
    assert_eq!(
        ProviderRepo::new(db.clone())
            .list(Some(AgentId::Codex))
            .unwrap(),
        Vec::<Provider>::new()
    );

    let start = prepared.runtime_material().start_spec(None);
    assert_eq!(start.profile_id, prepared.profile().id);
    assert_eq!(start.port, 0);
    assert_eq!(start.upstream.base_url, KIMI_CHAT_BASE_URL);
    assert_eq!(
        start.upstream.source_connection_id.as_deref(),
        Some("kimi-membership")
    );

    let generated = create_projection(&db, &prepared, 43121);
    assert_eq!(generated.agent_id, AgentId::Codex);
    assert_eq!(generated.settings_config["format"], "toml");
    assert_eq!(
        generated.settings_config["auth"]["OPENAI_API_KEY"]
            .as_str()
            .unwrap()
            .len(),
        47
    );
    assert!(generated.settings_config["content"]
        .as_str()
        .unwrap()
        .contains("http://127.0.0.1:43121/v1"));
    assert!(!serde_json::to_string(&generated)
        .unwrap()
        .contains("upstream-membership-secret"));

    let finalized = service.finalize(&prepared, 43121).unwrap();
    assert_eq!(finalized.status, AdapterProfileStatus::Active);
    assert_eq!(finalized.local_port, Some(43121));
    assert!(finalized.auto_start);
    assert!(!serde_json::to_string(&finalized)
        .unwrap()
        .contains("upstream-membership-secret"));

    let restorable = service.list_auto_start_profiles().unwrap();
    assert_eq!(restorable, vec![finalized.clone()]);
    let restored = service.resolve_restore_material(&finalized.id).unwrap();
    assert_eq!(restored.profile(), &finalized);
    assert_eq!(restored.runtime_material().start_spec(None).port, 43121);
    assert!(!format!("{restored:?}").contains("upstream-membership-secret"));
}

#[test]
fn retry_reuses_local_bearer_and_requests_provider_update_after_rebind() {
    let (_dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&kimi_source(
            "kimi-membership",
            "upstream-membership-secret",
        ))
        .unwrap();
    let service = AdapterBridgeService::new(db.clone());

    let first = service.prepare(&request("kimi-membership")).unwrap();
    let created = create_projection(&db, &first, 43121);
    let first_bearer = created.settings_config["auth"]["OPENAI_API_KEY"]
        .as_str()
        .unwrap()
        .to_owned();
    service.finalize(&first, 43121).unwrap();

    let retry = service.prepare(&request("kimi-membership")).unwrap();
    match retry.provider_projection(43121).unwrap() {
        AdapterBridgeProviderProjection::None => {}
        other => panic!("same port must not rewrite provider: {other:?}"),
    }
    let input = match retry.provider_projection(43122).unwrap() {
        AdapterBridgeProviderProjection::Update(input) => input,
        other => panic!("new port must update provider: {other:?}"),
    };
    assert_eq!(
        input.settings_config["auth"]["OPENAI_API_KEY"],
        first_bearer
    );
    let updated = ProviderService::new(db.clone()).update(&input).unwrap();
    assert!(updated.settings_config["content"]
        .as_str()
        .unwrap()
        .contains("127.0.0.1:43122/v1"));
    let finalized = service.finalize(&retry, 43122).unwrap();
    assert_eq!(finalized.local_port, Some(43122));
}

#[test]
fn invalid_source_and_provider_collision_fail_without_creating_profile() {
    let (_dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&kimi_source("missing-key", ""))
        .unwrap();
    let service = AdapterBridgeService::new(db.clone());
    assert_eq!(
        service.prepare(&request("missing-key")).unwrap_err().code(),
        "invalid_arg"
    );
    assert!(AdapterProfileRepo::new(db.clone())
        .list(None, None, None)
        .unwrap()
        .is_empty());

    let source = kimi_source("colliding-source", "upstream-membership-secret");
    ProviderRepo::new(db.clone()).create(&source).unwrap();
    let collision = Provider {
        id: stable_id("codex-kimi-adapter-bridge", &source.id),
        agent_id: AgentId::Codex,
        name: "user provider".into(),
        settings_config: json!({"format": "toml", "content": "model = 'user'"}),
        meta: json!({"preset": "openai-compatible"}),
        is_current: false,
        created_at: "now".into(),
        updated_at: "now".into(),
    };
    ProviderRepo::new(db.clone()).create(&collision).unwrap();
    assert_eq!(
        service.prepare(&request(&source.id)).unwrap_err().code(),
        "adapter.provider_conflict"
    );
    assert!(AdapterProfileRepo::new(db)
        .list(None, None, None)
        .unwrap()
        .is_empty());
}

#[test]
fn revalidate_projection_rejects_provider_that_became_user_owned_after_prepare() {
    let (_dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&kimi_source(
            "kimi-membership",
            "upstream-membership-secret",
        ))
        .unwrap();
    let service = AdapterBridgeService::new(db.clone());
    let prepared = service.prepare(&request("kimi-membership")).unwrap();
    let provider_id = prepared
        .profile()
        .generated_provider_id
        .as_deref()
        .unwrap()
        .to_owned();

    // Simulates an external process creating the deterministic id while the
    // listener bind/health phase is in progress. The late projection check
    // must fail before any update can overwrite this user-owned row.
    let user_owned = Provider {
        id: provider_id.clone(),
        agent_id: AgentId::Codex,
        name: "user-owned collision".into(),
        settings_config: json!({"format": "toml", "content": "model = 'user'"}),
        meta: json!({"preset": "openai-compatible"}),
        is_current: false,
        created_at: "now".into(),
        updated_at: "now".into(),
    };
    ProviderRepo::new(db.clone()).create(&user_owned).unwrap();

    let error = service
        .revalidate_provider_projection(&prepared, 43121)
        .unwrap_err();
    assert_eq!(error.code(), "adapter.provider_conflict");
    assert_eq!(
        ProviderRepo::new(db)
            .get_by_id(&provider_id)
            .unwrap()
            .unwrap(),
        user_owned,
        "late revalidation must not overwrite a user-owned collision"
    );
}

#[test]
fn auto_start_and_attention_are_profile_only_state_transitions() {
    let (_dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&kimi_source(
            "kimi-membership",
            "upstream-membership-secret",
        ))
        .unwrap();
    let service = AdapterBridgeService::new(db.clone());
    let prepared = service.prepare(&request("kimi-membership")).unwrap();
    let original = prepared.profile().clone();

    let disabled = service.set_auto_start(&original.id, false).unwrap();
    assert!(!disabled.auto_start);
    assert!(service.list_auto_start_profiles().unwrap().is_empty());
    let attention = service
        .mark_needs_attention(&original.id, "adapter.port_in_use")
        .unwrap();
    assert_eq!(attention.status, AdapterProfileStatus::NeedsAttention);
    assert_eq!(
        attention.last_error_code.as_deref(),
        Some("adapter.port_in_use")
    );
    assert!(ProviderRepo::new(db)
        .list(Some(AgentId::Codex))
        .unwrap()
        .is_empty());
}

#[test]
fn retryable_restore_failure_stays_auto_start_eligible_and_prepare_retries() {
    let (_dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&kimi_source(
            "kimi-membership",
            "upstream-membership-secret",
        ))
        .unwrap();
    let service = AdapterBridgeService::new(db.clone());
    let prepared = service.prepare(&request("kimi-membership")).unwrap();
    create_projection(&db, &prepared, 43121);
    let active = service.finalize(&prepared, 43121).unwrap();

    let retryable = service
        .mark_retryable(&active.id, "adapter.port_in_use")
        .unwrap();
    assert_eq!(retryable.status, AdapterProfileStatus::Active);
    assert_eq!(
        retryable.last_error_code.as_deref(),
        Some("retryable:adapter.port_in_use")
    );
    assert_eq!(
        service
            .list_auto_start_profiles()
            .unwrap()
            .into_iter()
            .map(|profile| profile.id)
            .collect::<Vec<_>>(),
        vec![active.id.clone()]
    );
    assert_eq!(
        service
            .resolve_restore_material(&active.id)
            .unwrap()
            .profile(),
        &retryable
    );

    let retried = service.prepare(&request("kimi-membership")).unwrap();
    assert_eq!(retried.profile().status, AdapterProfileStatus::Active);
    let finalized = service.finalize(&retried, 43121).unwrap();
    assert_eq!(finalized.last_error_code, None);
}

#[test]
fn successful_restore_clears_only_retryable_marker() {
    let (_dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&kimi_source(
            "kimi-membership",
            "upstream-membership-secret",
        ))
        .unwrap();
    let service = AdapterBridgeService::new(db.clone());
    let prepared = service.prepare(&request("kimi-membership")).unwrap();
    create_projection(&db, &prepared, 43121);
    let active = service.finalize(&prepared, 43121).unwrap();

    service
        .mark_retryable(&active.id, "adapter.bridge_upstream_auth")
        .unwrap();
    let cleared = service.clear_retryable_error(&active.id).unwrap();
    assert_eq!(cleared.status, AdapterProfileStatus::Active);
    assert_eq!(cleared.last_error_code, None);

    let attention = service
        .mark_needs_attention(&active.id, "adapter.bridge_rollback")
        .unwrap();
    assert_eq!(
        service.clear_retryable_error(&active.id).unwrap(),
        attention,
        "a successful restore must never erase a non-retryable attention signal"
    );
}

#[tokio::test]
async fn bound_health_rejects_upstream_auth_before_a_provider_switch() {
    let (upstream_port, upstream_task) = health_upstream(StatusCode::UNAUTHORIZED).await;
    let material = AdapterBridgeRuntimeMaterial {
        profile_id: "health-profile".into(),
        source_connection_id: "kimi-membership".into(),
        preferred_port: None,
        upstream_base_url: format!("http://127.0.0.1:{upstream_port}"),
        upstream_auth: ResolvedAuth::bearer("upstream-secret"),
        local_bearer: "local-secret".into(),
    };
    let host = crate::bridge::BridgeRuntimeHost::new();
    let runtime = host.start(material.start_spec(Some(0))).await.unwrap();

    let error = material
        .verify_bound_health(runtime.port)
        .await
        .unwrap_err();
    assert_eq!(error.code(), "adapter.bridge_upstream_auth");

    host.shutdown().await.unwrap();
    upstream_task.abort();
}

#[test]
fn restore_uses_a_rotated_source_key_without_changing_the_local_bearer() {
    let (_dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&kimi_source("kimi-membership", "original-upstream-secret"))
        .unwrap();
    let service = AdapterBridgeService::new(db.clone());
    let prepared = service.prepare(&request("kimi-membership")).unwrap();
    let generated = create_projection(&db, &prepared, 43121);
    let local_bearer = generated.settings_config["auth"]["OPENAI_API_KEY"]
        .as_str()
        .unwrap()
        .to_owned();
    let profile = service.finalize(&prepared, 43121).unwrap();

    let mut rotated = ProviderRepo::new(db.clone())
        .get_by_id("kimi-membership")
        .unwrap()
        .unwrap();
    rotated.settings_config = json!({"apiKey": "rotated-upstream-secret"});
    rotated.updated_at = "rotated".into();
    ProviderRepo::new(db.clone()).update(&rotated).unwrap();

    let restored = service.resolve_restore_material(&profile.id).unwrap();
    let start = restored.runtime_material().start_spec(None);
    assert_eq!(start.local_token, local_bearer);
    assert_eq!(start.upstream.auth.token(), "rotated-upstream-secret");
    assert!(!format!("{restored:?}").contains("rotated-upstream-secret"));
}

#[test]
fn malformed_generated_provider_version_fails_closed() {
    let (_dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&kimi_source(
            "kimi-membership",
            "upstream-membership-secret",
        ))
        .unwrap();
    let service = AdapterBridgeService::new(db.clone());
    let prepared = service.prepare(&request("kimi-membership")).unwrap();
    let provider = create_projection(&db, &prepared, 43121);
    let mut version_mutation = provider.clone();
    version_mutation.meta["adapterRuleVersion"] = json!(2);
    version_mutation.updated_at = "bad-version".into();
    ProviderRepo::new(db.clone())
        .update(&version_mutation)
        .unwrap();
    assert_eq!(
        service
            .prepare(&request("kimi-membership"))
            .unwrap_err()
            .code(),
        "adapter.provider_conflict"
    );
}

#[test]
fn bridge_remove_preflight_requires_owned_non_current_provider_then_removes_profile() {
    let (_dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&kimi_source(
            "kimi-membership",
            "upstream-membership-secret",
        ))
        .unwrap();
    let service = AdapterBridgeService::new(db.clone());
    let prepared = service.prepare(&request("kimi-membership")).unwrap();
    let generated = create_projection(&db, &prepared, 43121);
    let profile = service.finalize(&prepared, 43121).unwrap();

    let removal = service.preflight_remove(&profile.id).unwrap();
    assert_eq!(removal.profile(), &profile);
    assert_eq!(removal.generated_provider_id(), Some(generated.id.as_str()));
    assert!(removal.recovery_input().is_some());

    ProviderRepo::new(db.clone()).delete(&generated.id).unwrap();
    service.complete_remove(&removal).unwrap();
    assert!(AdapterProfileRepo::new(db)
        .get(&profile.id)
        .unwrap()
        .is_none());
}

#[test]
fn bridge_remove_preflight_rejects_current_or_malformed_generated_provider() {
    let (_dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&kimi_source(
            "kimi-membership",
            "upstream-membership-secret",
        ))
        .unwrap();
    let service = AdapterBridgeService::new(db.clone());
    let prepared = service.prepare(&request("kimi-membership")).unwrap();
    let generated = create_projection(&db, &prepared, 43121);
    let profile = service.finalize(&prepared, 43121).unwrap();

    let mut current = generated.clone();
    current.is_current = true;
    ProviderRepo::new(db.clone()).update(&current).unwrap();
    assert_eq!(
        service.preflight_remove(&profile.id).unwrap_err().code(),
        "unsupported"
    );

    current.is_current = false;
    current.meta["adapterRuleVersion"] = json!(2);
    ProviderRepo::new(db).update(&current).unwrap();
    assert_eq!(
        service.preflight_remove(&profile.id).unwrap_err().code(),
        "adapter.provider_conflict"
    );
}
