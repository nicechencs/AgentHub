use super::*;

use crate::models::{
    Account, AccountKind, AdapterProfile, AdapterProfileMode, AdapterProfileStatus, AdapterRoute,
    AdapterSourceKind, AdapterSourceProduct, AdapterTargetProtocol, AdapterUpstreamTransport,
    Provider, RouteDownstreamDialect, RouteDownstreamSurface, RouteSchedulePolicy,
    FEATURE_MIXED_PROVIDER_POOL, FEATURE_ROUTE_INDEX_V2, FEATURE_ROUTE_POOL_V2, LOCAL_BRIDGE_EDGES,
};
use crate::services::{ProviderService, RoutePoolService};
use crate::storage::{AccountRepo, AdapterProfileRepo, ProviderRepo, RoutePoolRepo};
use axum::body::Body;
use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use futures_util::stream;
use std::sync::{Arc, Mutex};
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

async fn redirecting_health_upstream() -> (
    u16,
    Arc<Mutex<Option<String>>>,
    tokio::task::JoinHandle<()>,
    tokio::task::JoinHandle<()>,
) {
    let forwarded_key = Arc::new(Mutex::new(None));
    let target_listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let target_port = target_listener.local_addr().unwrap().port();
    let target_state = forwarded_key.clone();
    let target_task = tokio::spawn(async move {
        async fn target(
            State(forwarded_key): State<Arc<Mutex<Option<String>>>>,
            headers: HeaderMap,
        ) -> StatusCode {
            let key = headers
                .get("x-api-key")
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned);
            *forwarded_key.lock().unwrap() = key;
            StatusCode::OK
        }

        axum::serve(
            target_listener,
            Router::new()
                .route("/models", get(target))
                .with_state(target_state),
        )
        .await
        .unwrap();
    });

    let redirect_listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let redirect_port = redirect_listener.local_addr().unwrap().port();
    let location = format!("http://127.0.0.1:{target_port}/models");
    let redirect_task = tokio::spawn(async move {
        let app = Router::new().route(
            "/models",
            get(move || {
                let location = location.clone();
                async move {
                    Response::builder()
                        .status(StatusCode::FOUND)
                        .header(header::LOCATION, location)
                        .body(Body::empty())
                        .unwrap()
                }
            }),
        );
        axum::serve(redirect_listener, app).await.unwrap();
    });

    (redirect_port, forwarded_key, redirect_task, target_task)
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

fn kimi_coding_live_import(id: &str, api_key: &str) -> Provider {
    Provider {
        id: id.into(),
        agent_id: AgentId::Kimi,
        name: "Kimi coding live import".into(),
        settings_config: json!({
            "apiKey": api_key,
            "baseUrl": "https://api.kimi.com/coding/v1",
        }),
        meta: json!({}),
        is_current: false,
        created_at: "now".into(),
        updated_at: "now".into(),
    }
}

fn kimi_account(id: &str, api_key: &str, kind: AccountKind) -> Account {
    Account {
        id: id.into(),
        agent_id: AgentId::Kimi,
        kind,
        label: "Kimi Code membership".into(),
        credentials: json!({
            "format": if kind == AccountKind::ApiKey { "api_key" } else { "oauth" },
            "api_key": if kind == AccountKind::ApiKey { api_key } else { "" },
            "provider": "kimi-code-membership",
        }),
        extra: json!({}),
        status: "active".into(),
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

fn account_request(source_id: &str) -> AdapterBridgePrepareRequest {
    AdapterBridgePrepareRequest {
        source_kind: AdapterSourceKind::Account,
        source_id: source_id.into(),
        target_agent_id: AgentId::Codex,
        auto_start: true,
    }
}

fn grok_codex_account_request(source_id: &str) -> AdapterBridgePrepareRequest {
    AdapterBridgePrepareRequest {
        source_kind: AdapterSourceKind::Account,
        source_id: source_id.into(),
        target_agent_id: AgentId::Codex,
        auto_start: true,
    }
}

fn grok_claude_account_request(source_id: &str) -> AdapterBridgePrepareRequest {
    AdapterBridgePrepareRequest {
        source_kind: AdapterSourceKind::Account,
        source_id: source_id.into(),
        target_agent_id: AgentId::Claude,
        auto_start: true,
    }
}

fn grok_subscription_account(id: &str, access_token: &str) -> Account {
    Account {
        id: id.into(),
        agent_id: AgentId::Grok,
        kind: AccountKind::Oauth,
        label: "Grok subscription".into(),
        credentials: json!({
            "format": "oauth",
            "access_token": access_token,
            "refresh_token": "grok-refresh-secret"
        }),
        extra: json!({}),
        status: "active".into(),
        is_current: false,
        created_at: "now".into(),
        updated_at: "now".into(),
    }
}

fn persist_mutated_provider(db: &Database, mut provider: Provider) {
    provider.updated_at = "mutated".into();
    ProviderRepo::new(db.clone()).update(&provider).unwrap();
}

#[test]
fn grok_subscription_prepare_uses_xai_chat_and_projects_only_loopback() {
    let (_dir, db) = test_db();
    AccountRepo::new(db.clone())
        .create(&grok_subscription_account(
            "grok-subscription",
            "grok-upstream-secret",
        ))
        .unwrap();
    let service = AdapterBridgeService::new(db);
    let prepared = service
        .prepare(&grok_claude_account_request("grok-subscription"))
        .unwrap();
    let spec = prepared.runtime_material().start_spec(Some(0));
    assert_eq!(
        spec.upstream.base_url,
        crate::bridge::grok_cli::GROK_CLI_PROXY_BASE_URL
    );
    assert_eq!(spec.upstream.model.as_deref(), Some("grok-4.5"));
    assert_eq!(
        spec.upstream.protocol,
        BridgeUpstreamProtocol::XaiResponsesOauth
    );
    assert_eq!(spec.upstream.local_surface, BridgeLocalSurface::Messages);
    assert_eq!(spec.upstream.auth.token(), "grok-upstream-secret");

    let projection = prepared.provider_projection(43123).unwrap();
    let input = match projection {
        AdapterBridgeProviderProjection::Create(input) => input,
        other => panic!("expected create projection, got {other:?}"),
    };
    let serialized = serde_json::to_string(&input.settings_config).unwrap();
    assert!(serialized.contains("127.0.0.1:43123"));
    assert!(!serialized.contains("grok-upstream-secret"));
    assert!(!serialized.contains("grok-refresh-secret"));
}

#[test]
fn grok_subscription_prepare_codex_uses_xai_chat_and_codex_toml() {
    let (_dir, db) = test_db();
    AccountRepo::new(db.clone())
        .create(&grok_subscription_account(
            "grok-subscription",
            "grok-upstream-secret",
        ))
        .unwrap();
    let service = AdapterBridgeService::new(db);
    let prepared = service
        .prepare(&grok_codex_account_request("grok-subscription"))
        .unwrap();
    let spec = prepared.runtime_material().start_spec(Some(0));
    assert_eq!(
        spec.upstream.base_url,
        crate::bridge::grok_cli::GROK_CLI_PROXY_BASE_URL
    );
    assert_eq!(spec.upstream.model.as_deref(), Some("grok-4.5"));
    assert_eq!(
        spec.upstream.protocol,
        BridgeUpstreamProtocol::XaiResponsesOauth
    );
    assert_eq!(spec.upstream.local_surface, BridgeLocalSurface::Responses);
    assert_eq!(spec.upstream.auth.token(), "grok-upstream-secret");

    let projection = prepared.provider_projection(43123).unwrap();
    let input = match projection {
        AdapterBridgeProviderProjection::Create(input) => input,
        other => panic!("expected create projection, got {other:?}"),
    };
    assert_eq!(input.agent_id, AgentId::Codex);
    let serialized = serde_json::to_string(&input.settings_config).unwrap();
    assert!(serialized.contains("127.0.0.1:43123"));
    assert!(serialized.contains("agenthub_grok_bridge"));
    assert!(!serialized.contains("grok-upstream-secret"));
    assert!(!serialized.contains("grok-refresh-secret"));
}

#[test]
fn oauth_local_bridge_projection_reuses_source_account_and_stays_non_current() {
    let cases: [(&str, Account, AdapterBridgePrepareRequest); 3] = [
        (
            "grok-claude",
            grok_subscription_account("grok-subscription", "grok-upstream-secret"),
            grok_claude_account_request("grok-subscription"),
        ),
        (
            "grok-codex",
            grok_subscription_account("grok-subscription", "grok-upstream-secret"),
            grok_codex_account_request("grok-subscription"),
        ),
        (
            "codex-claude",
            codex_subscription_account("codex-subscription", "codex-upstream-access-secret"),
            codex_claude_request("codex-subscription"),
        ),
    ];
    for (label, account, request) in cases {
        let source_id = account.id.clone();
        let source_agent = account.agent_id;
        let (_dir, db) = test_db();
        AccountRepo::new(db.clone()).create(&account).unwrap();
        let before = AccountRepo::new(db.clone()).list(None).unwrap();
        assert_eq!(before.len(), 1, "{label}");
        let service = AdapterBridgeService::new(db.clone());
        let prepared = service.prepare(&request).unwrap();
        assert_eq!(prepared.profile().source_id, source_id, "{label}");
        assert_eq!(
            prepared.profile().source_kind,
            AdapterSourceKind::Account,
            "{label}"
        );
        let generated = create_projection(&db, &prepared, 43121);
        assert!(!generated.is_current, "{label}");
        service.finalize(&prepared, 43121).unwrap();
        let after = AccountRepo::new(db.clone()).list(None).unwrap();
        assert_eq!(after.len(), 1, "{label}");
        assert_eq!(after[0].id, source_id, "{label}");
        assert_eq!(after[0].agent_id, source_agent, "{label}");
        assert_eq!(after[0].kind, AccountKind::Oauth, "{label}");
        let stored = ProviderRepo::new(db)
            .get_by_id(&generated.id)
            .unwrap()
            .unwrap();
        assert!(!stored.is_current, "{label}");
        assert_eq!(stored.agent_id, request.target_agent_id, "{label}");
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
    assert_eq!(start.upstream.source_id.as_deref(), Some("kimi-membership"));

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
fn account_prepare_projects_without_plaintext_and_oauth_is_rejected() {
    let (_dir, db) = test_db();
    AccountRepo::new(db.clone())
        .create(&kimi_account(
            "kimi-account",
            "account-upstream-secret",
            AccountKind::ApiKey,
        ))
        .unwrap();
    let service = AdapterBridgeService::new(db.clone());
    let prepared = service.prepare(&account_request("kimi-account")).unwrap();
    assert_eq!(prepared.profile().source_kind, AdapterSourceKind::Account);
    assert!(!format!("{prepared:?}").contains("account-upstream-secret"));

    let generated = create_projection(&db, &prepared, 43123);
    assert_eq!(generated.meta["adapterSourceRef"]["kind"], "account");
    assert!(!serde_json::to_string(&generated)
        .unwrap()
        .contains("account-upstream-secret"));

    let (_oauth_dir, oauth_db) = test_db();
    AccountRepo::new(oauth_db.clone())
        .create(&kimi_account(
            "kimi-oauth-account",
            "oauth-secret",
            AccountKind::Oauth,
        ))
        .unwrap();
    let oauth_service = AdapterBridgeService::new(oauth_db);
    assert_eq!(
        oauth_service
            .prepare(&account_request("kimi-oauth-account"))
            .unwrap_err()
            .code(),
        "unsupported"
    );
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
fn prepare_rebuilds_leftover_incomplete_generated_provider() {
    let (_dir, db) = test_db();
    AccountRepo::new(db.clone())
        .create(&codex_subscription_account(
            "codex-subscription",
            "codex-upstream-access-secret",
        ))
        .unwrap();
    let service = AdapterBridgeService::new(db.clone());
    let first = service
        .prepare(&codex_claude_request("codex-subscription"))
        .unwrap();
    let provider_id = first
        .profile()
        .generated_provider_id
        .as_deref()
        .expect("generated id")
        .to_owned();
    let profile_id = first.profile().id.clone();
    let incomplete = Provider {
        id: provider_id.clone(),
        agent_id: AgentId::Claude,
        name: "Codex Subscription Bridge".into(),
        settings_config: json!({ "env": {} }),
        meta: first
            .provider_projection(40661)
            .ok()
            .and_then(|p| match p {
                AdapterBridgeProviderProjection::Create(input) => Some(input.meta),
                _ => None,
            })
            .unwrap_or_else(|| json!({ "generatedBy": "adapter" })),
        is_current: false,
        created_at: "now".into(),
        updated_at: "now".into(),
    };
    ProviderRepo::new(db.clone()).create(&incomplete).unwrap();
    let mut profile = AdapterProfileRepo::new(db.clone())
        .get(&profile_id)
        .unwrap()
        .unwrap();
    profile.status = AdapterProfileStatus::Active;
    profile.local_port = Some(40661);
    profile.last_error_code = Some("retryable:adapter.bridge_restore_source".into());
    AdapterProfileRepo::new(db.clone())
        .update(&profile)
        .unwrap();

    let prepared = service
        .prepare(&codex_claude_request("codex-subscription"))
        .expect("leftover incomplete projection must rebuild, not fail closed");
    assert!(prepared.runtime_material().local_bearer.starts_with("ahb_"));
    assert!(prepared.profile().local_port.is_none());
    assert_ne!(prepared.runtime_material().local_bearer, "***");
}

#[test]
fn revalidate_projection_rebuilds_incomplete_generated_provider() {
    let (_dir, db) = test_db();
    AccountRepo::new(db.clone())
        .create(&codex_subscription_account(
            "codex-subscription",
            "codex-upstream-access-secret",
        ))
        .unwrap();
    let service = AdapterBridgeService::new(db.clone());
    let prepared = service
        .prepare(&codex_claude_request("codex-subscription"))
        .unwrap();
    let provider_id = prepared
        .profile()
        .generated_provider_id
        .as_deref()
        .expect("generated id")
        .to_owned();
    let incomplete = match prepared.provider_projection(43121).unwrap() {
        AdapterBridgeProviderProjection::Create(input) => Provider {
            id: input.id,
            agent_id: input.agent_id,
            name: input.name,
            settings_config: json!({}),
            meta: input.meta,
            is_current: false,
            created_at: "now".into(),
            updated_at: "now".into(),
        },
        other => panic!("expected create projection, got {other:?}"),
    };
    assert_eq!(incomplete.id, provider_id);
    ProviderRepo::new(db.clone()).create(&incomplete).unwrap();

    let projection = service
        .revalidate_provider_projection(&prepared, 43121)
        .unwrap();
    let input = match projection {
        AdapterBridgeProviderProjection::Update(input) => input,
        other => panic!("incomplete generated login must be rewritten, got {other:?}"),
    };
    assert_eq!(
        input.settings_config["env"]["ANTHROPIC_BASE_URL"],
        "http://127.0.0.1:43121"
    );
    let token = input.settings_config["env"]["ANTHROPIC_AUTH_TOKEN"]
        .as_str()
        .unwrap_or("");
    assert!(
        token.starts_with("ahb_"),
        "rebuild must persist the local token"
    );
    assert!(!token.is_empty());
    assert_eq!(
        ProviderRepo::new(db)
            .get_by_id(&provider_id)
            .unwrap()
            .unwrap()
            .settings_config,
        json!({}),
        "revalidate must not write until persist"
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
fn failed_first_apply_does_not_remain_applying() {
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
    assert_eq!(prepared.profile().local_port, None);

    let failed = service
        .mark_retryable(&prepared.profile().id, "adapter.bridge_projection")
        .unwrap();
    assert_eq!(failed.status, AdapterProfileStatus::NeedsAttention);
    assert_eq!(
        failed.last_error_code.as_deref(),
        Some("retryable:adapter.bridge_projection")
    );
    assert_eq!(failed.local_port, None);
    assert!(service.list_auto_start_profiles().unwrap().is_empty());
}

fn applying_profile_for_rule(rule: &super::CodexBridgeRule) -> AdapterProfile {
    let source_kind = if rule.mode == AdapterProfileMode::Oauth {
        AdapterSourceKind::Account
    } else {
        AdapterSourceKind::Provider
    };
    AdapterProfile {
        id: format!("{}-audit", rule.profile_prefix),
        name: rule.profile_name.into(),
        source_kind,
        source_id: "audit-source".into(),
        target_agent_id: rule.target_agent,
        route: AdapterRoute::LocalBridge,
        mode: rule.mode,
        status: AdapterProfileStatus::Applying,
        rule_id: rule.rule_id.into(),
        rule_version: super::RULE_VERSION.into(),
        generated_provider_id: Some(format!("{}-generated", rule.provider_prefix)),
        local_port: None,
        auto_start: true,
        last_error_code: None,
        created_at: "now".into(),
        updated_at: "now".into(),
    }
}

#[test]
fn live_bridge_rules_match_local_bridge_catalog() {
    use std::collections::BTreeSet;

    let live_ids: BTreeSet<&str> = super::LIVE_BRIDGE_RULES
        .iter()
        .map(|rule| rule.rule_id)
        .collect();
    for edge in LOCAL_BRIDGE_EDGES {
        if !(edge.can_apply && edge.gates.all_passed()) {
            assert!(
                !live_ids.contains(edge.rule_id),
                "closed catalog edge {} must not be a live writer until can_apply opens",
                edge.rule_id
            );
            continue;
        }
        let rule = super::LIVE_BRIDGE_RULES
            .iter()
            .find(|rule| rule.rule_id == edge.rule_id)
            .unwrap_or_else(|| {
                panic!(
                    "applyable catalog edge {} missing from LIVE_BRIDGE_RULES",
                    edge.rule_id
                )
            });
        assert_eq!(rule.source, edge.source, "{}", edge.rule_id);
        assert_eq!(rule.target_agent, edge.target, "{}", edge.rule_id);
        assert_eq!(rule.default_model, edge.default_model, "{}", edge.rule_id);
        let expected_surface = match edge.protocol {
            AdapterTargetProtocol::AnthropicMessages => super::BridgeLocalSurface::Messages,
            AdapterTargetProtocol::OpenAiResponses => super::BridgeLocalSurface::Responses,
            AdapterTargetProtocol::OpenAiChatCompletions => {
                super::BridgeLocalSurface::ChatCompletions
            }
            other => panic!(
                "{}: local-bridge protocol {other:?} is not a wire surface",
                edge.rule_id
            ),
        };
        let expected_protocol = match edge.transport {
            AdapterUpstreamTransport::LocalBridgeChatCompletions => {
                super::BridgeUpstreamProtocol::OpenAiChatCompletions
            }
            AdapterUpstreamTransport::LocalBridgeAnthropicMessages => {
                super::BridgeUpstreamProtocol::AnthropicMessages
            }
            AdapterUpstreamTransport::CodexResponsesOauth => {
                super::BridgeUpstreamProtocol::CodexResponsesOauth
            }
            AdapterUpstreamTransport::XaiResponsesOauth => {
                super::BridgeUpstreamProtocol::XaiResponsesOauth
            }
            other => panic!(
                "{}: transport {other:?} is not a live upstream",
                edge.rule_id
            ),
        };
        assert_eq!(rule.local_surface, expected_surface, "{}", edge.rule_id);
        assert_eq!(rule.protocol, expected_protocol, "{}", edge.rule_id);
    }

    for rule in super::LIVE_BRIDGE_RULES {
        assert!(
            LOCAL_BRIDGE_EDGES
                .iter()
                .any(|edge| edge.rule_id == rule.rule_id
                    && edge.can_apply
                    && edge.gates.all_passed()),
            "live writer {} has no applyable catalog row",
            rule.rule_id
        );
    }
}

#[test]
fn failed_first_apply_does_not_remain_applying_for_every_live_bridge_rule() {
    for rule in super::LIVE_BRIDGE_RULES {
        let (_dir, db) = test_db();
        let profile = applying_profile_for_rule(rule);
        AdapterProfileRepo::new(db.clone())
            .create(&profile)
            .unwrap();
        let failed = AdapterBridgeService::new(db)
            .mark_retryable(&profile.id, "adapter.bridge_projection")
            .unwrap();
        assert_eq!(
            failed.status,
            AdapterProfileStatus::NeedsAttention,
            "first-time apply for {} must not stay applying without a port",
            rule.rule_id
        );
        assert_eq!(failed.local_port, None);
        assert_eq!(
            failed.last_error_code.as_deref(),
            Some("retryable:adapter.bridge_projection")
        );
    }
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
}

#[test]
fn restored_port_projection_and_persist_realign_active_profile() {
    let (_dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&kimi_source(
            "kimi-membership",
            "upstream-membership-secret",
        ))
        .unwrap();
    let service = AdapterBridgeService::new(db.clone());
    let prepared = service.prepare(&request("kimi-membership")).unwrap();
    let local_bearer = prepared
        .runtime_material()
        .start_spec(None)
        .local_token
        .clone();
    create_projection(&db, &prepared, 43121);
    let active = service.finalize(&prepared, 43121).unwrap();
    service
        .mark_retryable(&active.id, "adapter.port_in_use")
        .unwrap();

    let (input, was_current) = service
        .projection_for_restored_port(&active.id, 43155)
        .unwrap();
    assert!(!was_current);
    assert!(!input.is_current);
    assert_eq!(input.id, active.generated_provider_id.as_deref().unwrap());
    let content = input
        .settings_config
        .get("content")
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    assert!(
        content.contains("127.0.0.1:43155"),
        "projection toml must target rebound port: {content}"
    );
    assert_eq!(
        input
            .settings_config
            .pointer("/auth/OPENAI_API_KEY")
            .and_then(|value| value.as_str()),
        Some(local_bearer.as_str())
    );

    ProviderService::new(db.clone()).update(&input).unwrap();
    let persisted = service.persist_restored_port(&active.id, 43155).unwrap();
    assert_eq!(persisted.local_port, Some(43155));
    assert_eq!(persisted.last_error_code, None);
    assert_eq!(persisted.status, AdapterProfileStatus::Active);

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
    let mut material = AdapterBridgeRuntimeMaterial {
        profile_id: "health-profile".into(),
        source_id: "kimi-membership".into(),
        preferred_port: None,
        upstream_base_url: format!("http://127.0.0.1:{upstream_port}"),
        upstream_model: "kimi-k2.5".into(),
        configured_listed_models: Vec::new(),
        context_window_tokens: None,
        protocol: crate::bridge::BridgeUpstreamProtocol::OpenAiChatCompletions,
        local_surface: BridgeLocalSurface::Responses,
        source: AdapterSourceProduct::KimiCodeMembership,
        target_agent: AgentId::Codex,
        downstream_dialect: RouteDownstreamDialect::Generic,
        upstream_auth: ResolvedAuth::bearer("upstream-secret"),
        local_bearer: "local-secret".into(),
        route_index: None,
        index_enabled: false,
        codex_ingress_grok_upstream: false,
        grok_ingress_codex_upstream: false,
        schedule_policy: Default::default(),
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

#[tokio::test]
async fn bound_health_does_not_forward_anthropic_key_across_redirect() {
    let (upstream_port, forwarded_key, redirect_task, target_task) =
        redirecting_health_upstream().await;
    let mut material = AdapterBridgeRuntimeMaterial {
        profile_id: "redirect-health-profile".into(),
        source_id: "anthropic-source".into(),
        preferred_port: None,
        upstream_base_url: format!("http://127.0.0.1:{upstream_port}"),
        upstream_model: "claude-test".into(),
        configured_listed_models: Vec::new(),
        context_window_tokens: None,
        protocol: BridgeUpstreamProtocol::AnthropicMessages,
        local_surface: BridgeLocalSurface::Messages,
        source: AdapterSourceProduct::AnthropicApi,
        target_agent: AgentId::Claude,
        downstream_dialect: RouteDownstreamDialect::Claude,
        upstream_auth: ResolvedAuth::bearer("anthropic-upstream-secret"),
        local_bearer: "local-secret".into(),
        route_index: None,
        index_enabled: false,
        codex_ingress_grok_upstream: false,
        grok_ingress_codex_upstream: false,
        schedule_policy: Default::default(),
    };
    let host = crate::bridge::BridgeRuntimeHost::new();
    let runtime = host.start(material.start_spec(Some(0))).await.unwrap();

    let error = material
        .verify_bound_health(runtime.port)
        .await
        .expect_err("a cross-origin redirect must not be followed");
    assert_eq!(error.code(), "adapter.bridge_health_upstream");
    assert_eq!(
        forwarded_key.lock().unwrap().as_deref(),
        None,
        "the redirect target must never receive the Anthropic API key"
    );

    host.shutdown().await.unwrap();
    redirect_task.abort();
    target_task.abort();
}

async fn models_response_server(
    chunks: Vec<axum::body::Bytes>,
    content_length: Option<usize>,
) -> (u16, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    let chunks = Arc::new(chunks);
    let app = Router::new().route(
        "/models",
        get(move || {
            let chunks = chunks.clone();
            async move {
                let chunks: Vec<_> = chunks.iter().cloned().collect();
                let body = Body::from_stream(stream::iter(
                    chunks.into_iter().map(Ok::<_, std::convert::Infallible>),
                ));
                let mut response = Response::new(body);
                if let Some(length) = content_length {
                    response
                        .headers_mut()
                        .insert(header::CONTENT_LENGTH, length.to_string().parse().unwrap());
                }
                response
            }
        }),
    );
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (port, task)
}

#[tokio::test]
async fn models_health_rejects_response_over_one_mib_from_content_length() {
    let oversized = axum::body::Bytes::from(vec![b'x'; MAX_UPSTREAM_MODELS_BODY_BYTES + 1]);
    let (port, task) =
        models_response_server(vec![oversized], Some(MAX_UPSTREAM_MODELS_BODY_BYTES + 1)).await;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let response = client
        .get(format!("http://127.0.0.1:{port}/models"))
        .send()
        .await
        .unwrap();
    let error = read_bounded_models_response(response).await.unwrap_err();
    assert_eq!(error.code(), "adapter.bridge_health_upstream_too_large");
    task.abort();
}

#[tokio::test]
async fn models_health_rejects_chunked_response_over_one_mib() {
    let chunks = vec![
        Ok::<_, std::convert::Infallible>(axum::body::Bytes::from(vec![b'x'; 700 * 1024])),
        Ok::<_, std::convert::Infallible>(axum::body::Bytes::from(vec![b'x'; 400 * 1024])),
    ];
    let chunks = chunks.into_iter().map(|chunk| chunk.unwrap()).collect();
    let (port, task) = models_response_server(chunks, None).await;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let response = client
        .get(format!("http://127.0.0.1:{port}/models"))
        .send()
        .await
        .unwrap();
    let error = read_bounded_models_response(response).await.unwrap_err();
    assert_eq!(error.code(), "adapter.bridge_health_upstream_too_large");
    task.abort();
}

#[tokio::test]
async fn codex_responses_health_probe_does_not_request_models() {
    let mut material = AdapterBridgeRuntimeMaterial {
        profile_id: "codex-health-profile".into(),
        source_id: "codex-subscription".into(),
        preferred_port: None,
        upstream_base_url: "http://127.0.0.1:9/should-not-be-called".into(),
        upstream_model: CODEX_DEFAULT_MODEL.into(),
        configured_listed_models: Vec::new(),
        context_window_tokens: None,
        protocol: BridgeUpstreamProtocol::CodexResponsesOauth,
        local_surface: BridgeLocalSurface::Messages,
        source: AdapterSourceProduct::CodexChatGptSubscription,
        target_agent: AgentId::Claude,
        downstream_dialect: RouteDownstreamDialect::Generic,
        upstream_auth: ResolvedAuth::bearer("codex-upstream-secret"),
        local_bearer: "local-secret".into(),
        route_index: None,
        index_enabled: false,
        codex_ingress_grok_upstream: false,
        grok_ingress_codex_upstream: false,
        schedule_policy: Default::default(),
    };
    let host = crate::bridge::BridgeRuntimeHost::new();
    let runtime = host.start(material.start_spec(Some(0))).await.unwrap();

    material
        .verify_bound_health(runtime.port)
        .await
        .expect("local health is sufficient for Codex Responses");

    host.shutdown().await.unwrap();
}

#[tokio::test]
async fn xai_responses_health_probe_does_not_request_models() {
    let mut material = AdapterBridgeRuntimeMaterial {
        profile_id: "grok-health-profile".into(),
        source_id: "grok-subscription".into(),
        preferred_port: None,
        upstream_base_url: "http://127.0.0.1:9/should-not-be-called".into(),
        upstream_model: crate::bridge::grok_cli::GROK_CLI_DEFAULT_MODEL.into(),
        configured_listed_models: Vec::new(),
        context_window_tokens: None,
        protocol: BridgeUpstreamProtocol::XaiResponsesOauth,
        local_surface: BridgeLocalSurface::Messages,
        source: AdapterSourceProduct::XaiGrokSubscription,
        target_agent: AgentId::Claude,
        downstream_dialect: RouteDownstreamDialect::Generic,
        upstream_auth: ResolvedAuth::bearer("grok-upstream-secret"),
        local_bearer: "local-secret".into(),
        route_index: None,
        index_enabled: false,
        codex_ingress_grok_upstream: false,
        grok_ingress_codex_upstream: false,
        schedule_policy: Default::default(),
    };
    let host = crate::bridge::BridgeRuntimeHost::new();
    let runtime = host.start(material.start_spec(Some(0))).await.unwrap();

    material
        .verify_bound_health(runtime.port)
        .await
        .expect("local health is sufficient for xAI Responses");

    host.shutdown().await.unwrap();
}

#[tokio::test]
async fn deepseek_health_probe_skips_upstream_models() {
    let mut material = AdapterBridgeRuntimeMaterial {
        profile_id: "deepseek-health-profile".into(),
        source_id: "deepseek-create".into(),
        preferred_port: None,
        upstream_base_url: "https://api.deepseek.com/anthropic".into(),
        upstream_model: "deepseek-chat".into(),
        configured_listed_models: Vec::new(),
        context_window_tokens: None,
        protocol: BridgeUpstreamProtocol::AnthropicMessages,
        local_surface: BridgeLocalSurface::Messages,
        source: AdapterSourceProduct::OpenaiApi,
        target_agent: AgentId::Claude,
        downstream_dialect: RouteDownstreamDialect::Generic,
        upstream_auth: ResolvedAuth::bearer("deepseek-upstream-secret"),
        local_bearer: "local-secret".into(),
        route_index: None,
        index_enabled: false,
        codex_ingress_grok_upstream: false,
        grok_ingress_codex_upstream: false,
        schedule_policy: Default::default(),
    };
    let host = crate::bridge::BridgeRuntimeHost::new();
    let runtime = host.start(material.start_spec(Some(0))).await.unwrap();

    material
        .verify_bound_health(runtime.port)
        .await
        .expect("local health is sufficient for DeepSeek Anthropic relay");

    host.shutdown().await.unwrap();
}

#[test]
fn start_spec_lists_codex_to_grok_dispatch_accepted_ids() {
    let material = AdapterBridgeRuntimeMaterial {
        profile_id: "codex-grok-models".into(),
        source_id: "codex-subscription".into(),
        preferred_port: None,
        upstream_base_url: CHATGPT_CODEX_BASE_URL.into(),
        upstream_model: String::new(),
        configured_listed_models: Vec::new(),
        context_window_tokens: None,
        protocol: BridgeUpstreamProtocol::CodexResponsesOauth,
        local_surface: BridgeLocalSurface::Responses,
        source: AdapterSourceProduct::CodexChatGptSubscription,
        target_agent: AgentId::Grok,
        downstream_dialect: RouteDownstreamDialect::Grok,
        upstream_auth: ResolvedAuth::bearer("codex-upstream-secret"),
        local_bearer: "local-secret".into(),
        route_index: None,
        index_enabled: false,
        codex_ingress_grok_upstream: false,
        grok_ingress_codex_upstream: false,
        schedule_policy: Default::default(),
    };
    assert_eq!(
        material.downstream_dialect(),
        RouteDownstreamDialect::Grok,
        "material carries the explicit downstream dialect"
    );
    assert_eq!(
        material.start_spec(Some(0)).downstream_responses_profile,
        Some(crate::bridge::DownstreamResponsesProfile::new(
            crate::bridge::ResponsesDialect::Grok,
        ))
    );
    let listed = material.start_spec(Some(0)).listed_models;
    assert!(!listed.is_empty());
    for model in &listed {
        assert!(
            !crate::bridge::protocol::responses::is_leftover_bridge_model(model),
            "leftover listed: {model}"
        );
    }
    assert_eq!(listed[0], "gpt-5.4");
}

#[test]
fn start_spec_lists_grok_default_when_mapping_entries_empty() {
    let material = AdapterBridgeRuntimeMaterial {
        profile_id: "grok-claude-models".into(),
        source_id: "grok-subscription".into(),
        preferred_port: None,
        upstream_base_url: crate::bridge::grok_cli::GROK_CLI_PROXY_BASE_URL.into(),
        upstream_model: crate::bridge::grok_cli::GROK_CLI_DEFAULT_MODEL.into(),
        configured_listed_models: Vec::new(),
        context_window_tokens: None,
        protocol: BridgeUpstreamProtocol::XaiResponsesOauth,
        local_surface: BridgeLocalSurface::Messages,
        source: AdapterSourceProduct::XaiGrokSubscription,
        target_agent: AgentId::Claude,
        downstream_dialect: RouteDownstreamDialect::Generic,
        upstream_auth: ResolvedAuth::bearer("grok-upstream-secret"),
        local_bearer: "local-secret".into(),
        route_index: None,
        index_enabled: false,
        codex_ingress_grok_upstream: false,
        grok_ingress_codex_upstream: false,
        schedule_policy: Default::default(),
    };
    assert_eq!(
        material.start_spec(Some(0)).listed_models,
        vec![crate::bridge::grok_cli::GROK_CLI_DEFAULT_MODEL.to_string()]
    );
}

#[test]
fn start_spec_lists_codex_to_kimi_dispatch_accepted_ids() {
    let material = AdapterBridgeRuntimeMaterial {
        profile_id: "codex-kimi-catalog-models".into(),
        source_id: "codex-subscription".into(),
        preferred_port: None,
        upstream_base_url: CHATGPT_CODEX_BASE_URL.into(),
        upstream_model: String::new(),
        configured_listed_models: Vec::new(),
        context_window_tokens: None,
        protocol: BridgeUpstreamProtocol::CodexResponsesOauth,
        local_surface: BridgeLocalSurface::ChatCompletions,
        source: AdapterSourceProduct::CodexChatGptSubscription,
        target_agent: AgentId::Kimi,
        downstream_dialect: RouteDownstreamDialect::Generic,
        upstream_auth: ResolvedAuth::bearer("codex-upstream-secret"),
        local_bearer: "local-secret".into(),
        route_index: None,
        index_enabled: false,
        codex_ingress_grok_upstream: false,
        grok_ingress_codex_upstream: false,
        schedule_policy: Default::default(),
    };
    let listed = material.start_spec(Some(0)).listed_models;
    assert_eq!(listed[0], "gpt-5.4");
    assert!(listed.iter().any(|model| model == "gpt-5.1-codex"));
    assert!(listed.iter().any(|model| model == "gpt-5"));
}

#[test]
fn start_spec_codex_to_kimi_configured_default_merges_into_catalog() {
    let material = AdapterBridgeRuntimeMaterial {
        profile_id: "codex-kimi-default-models".into(),
        source_id: "codex-subscription".into(),
        preferred_port: None,
        upstream_base_url: CHATGPT_CODEX_BASE_URL.into(),
        upstream_model: "gpt-5.4".into(),
        configured_listed_models: Vec::new(),
        context_window_tokens: None,
        protocol: BridgeUpstreamProtocol::CodexResponsesOauth,
        local_surface: BridgeLocalSurface::ChatCompletions,
        source: AdapterSourceProduct::CodexChatGptSubscription,
        target_agent: AgentId::Kimi,
        downstream_dialect: RouteDownstreamDialect::Generic,
        upstream_auth: ResolvedAuth::bearer("codex-upstream-secret"),
        local_bearer: "local-secret".into(),
        route_index: None,
        index_enabled: false,
        codex_ingress_grok_upstream: false,
        grok_ingress_codex_upstream: false,
        schedule_policy: Default::default(),
    };
    let listed = material.start_spec(Some(0)).listed_models;
    assert_eq!(listed[0], "gpt-5.4");
    assert!(listed.iter().any(|model| model == "gpt-5.1-codex"));
    assert!(listed.iter().any(|model| model == "gpt-5"));
}

#[test]
fn start_spec_lists_openai_to_codex_without_kimi_ids() {
    let material = AdapterBridgeRuntimeMaterial {
        profile_id: "openai-codex-models".into(),
        source_id: "openai-api".into(),
        preferred_port: None,
        upstream_base_url: OPENAI_CHAT_BASE_URL.into(),
        upstream_model: OPENAI_DEFAULT_MODEL.into(),
        configured_listed_models: Vec::new(),
        context_window_tokens: None,
        protocol: BridgeUpstreamProtocol::OpenAiChatCompletions,
        local_surface: BridgeLocalSurface::Responses,
        source: AdapterSourceProduct::OpenaiApi,
        target_agent: AgentId::Codex,
        downstream_dialect: RouteDownstreamDialect::Codex,
        upstream_auth: ResolvedAuth::bearer("openai-upstream-secret"),
        local_bearer: "local-secret".into(),
        route_index: None,
        index_enabled: false,
        codex_ingress_grok_upstream: false,
        grok_ingress_codex_upstream: false,
        schedule_policy: Default::default(),
    };
    let listed = material.start_spec(Some(0)).listed_models;
    assert_eq!(listed, vec![OPENAI_DEFAULT_MODEL.to_string()]);
    assert!(listed.iter().all(|model| !model.starts_with("kimi-")));
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

#[test]
fn prepare_accepts_coding_endpoint_without_preset() {
    let (_dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&kimi_coding_live_import(
            "kimi-live-import",
            "upstream-membership-secret",
        ))
        .unwrap();
    let service = AdapterBridgeService::new(db);

    let prepared = service.prepare(&request("kimi-live-import")).unwrap();
    assert_eq!(prepared.profile().route, AdapterRoute::LocalBridge);
    assert_eq!(prepared.profile().status, AdapterProfileStatus::Applying);
    assert!(!format!("{prepared:?}").contains("upstream-membership-secret"));
}

fn anthropic_source(id: &str, api_key: &str) -> Provider {
    Provider {
        id: id.into(),
        agent_id: AgentId::Claude,
        name: "Anthropic API".into(),
        settings_config: json!({"apiKey": api_key}),
        meta: json!({"preset": "anthropic"}),
        is_current: false,
        created_at: "now".into(),
        updated_at: "now".into(),
    }
}

fn anthropic_account(id: &str, api_key: &str) -> Account {
    Account {
        id: id.into(),
        agent_id: AgentId::Claude,
        kind: AccountKind::ApiKey,
        label: "Anthropic key".into(),
        credentials: json!({"format": "api_key", "api_key": api_key}),
        extra: json!({"provider": "anthropic"}),
        status: "active".into(),
        is_current: false,
        created_at: "now".into(),
        updated_at: "now".into(),
    }
}

fn openai_source(id: &str, api_key: &str) -> Provider {
    Provider {
        id: id.into(),
        agent_id: AgentId::Codex,
        name: "OpenAI API".into(),
        settings_config: json!({"apiKey": api_key}),
        meta: json!({"preset": "openai"}),
        is_current: false,
        created_at: "now".into(),
        updated_at: "now".into(),
    }
}

fn openai_account(id: &str, api_key: &str) -> Account {
    Account {
        id: id.into(),
        agent_id: AgentId::Claude,
        kind: AccountKind::ApiKey,
        label: "OpenAI key".into(),
        credentials: json!({"format": "api_key", "api_key": api_key}),
        extra: json!({"provider": "openai"}),
        status: "active".into(),
        is_current: false,
        created_at: "now".into(),
        updated_at: "now".into(),
    }
}

fn openai_request(source_kind: AdapterSourceKind, source_id: &str) -> AdapterBridgePrepareRequest {
    AdapterBridgePrepareRequest {
        source_kind,
        source_id: source_id.into(),
        target_agent_id: AgentId::Codex,
        auto_start: true,
    }
}

fn anthropic_request(
    source_kind: AdapterSourceKind,
    source_id: &str,
) -> AdapterBridgePrepareRequest {
    AdapterBridgePrepareRequest {
        source_kind,
        source_id: source_id.into(),
        target_agent_id: AgentId::Codex,
        auto_start: true,
    }
}

fn codex_claude_request(source_id: &str) -> AdapterBridgePrepareRequest {
    AdapterBridgePrepareRequest {
        source_kind: AdapterSourceKind::Account,
        source_id: source_id.into(),
        target_agent_id: AgentId::Claude,
        auto_start: true,
    }
}

fn codex_chat_request(source_id: &str, target: AgentId) -> AdapterBridgePrepareRequest {
    AdapterBridgePrepareRequest {
        source_kind: AdapterSourceKind::Account,
        source_id: source_id.into(),
        target_agent_id: target,
        auto_start: true,
    }
}

fn codex_subscription_account(id: &str, access_token: &str) -> Account {
    Account {
        id: id.into(),
        agent_id: AgentId::Codex,
        kind: AccountKind::Oauth,
        label: "Codex subscription".into(),
        credentials: json!({
            "format": "auth_json",
            "tokens": {
                "access_token": access_token,
                "refresh_token": "refresh-must-not-enter-bridge"
            }
        }),
        extra: json!({}),
        status: "active".into(),
        is_current: false,
        created_at: "now".into(),
        updated_at: "now".into(),
    }
}

#[test]
fn prepare_codex_subscription_projects_only_claude_loopback_env() {
    let (_dir, db) = test_db();
    AccountRepo::new(db.clone())
        .create(&codex_subscription_account(
            "codex-subscription",
            "codex-upstream-access-secret",
        ))
        .unwrap();
    let service = AdapterBridgeService::new(db.clone());

    let prepared = service
        .prepare(&codex_claude_request("codex-subscription"))
        .unwrap();
    assert_eq!(prepared.profile().target_agent_id, AgentId::Claude);
    assert_eq!(prepared.profile().mode, AdapterProfileMode::Oauth);
    assert_eq!(
        prepared.profile().rule_id,
        "codex-subscription-to-claude-responses-v1"
    );
    assert_eq!(
        prepared
            .runtime_material()
            .start_spec(None)
            .upstream
            .base_url,
        "https://chatgpt.com/backend-api/codex/"
    );
    assert_eq!(
        prepared
            .runtime_material()
            .start_spec(None)
            .upstream
            .model
            .as_deref(),
        Some("gpt-5.4")
    );
    assert_eq!(
        prepared
            .runtime_material()
            .start_spec(None)
            .upstream
            .protocol,
        BridgeUpstreamProtocol::CodexResponsesOauth
    );
    assert_eq!(
        prepared
            .runtime_material()
            .start_spec(None)
            .upstream
            .auth
            .token(),
        "codex-upstream-access-secret"
    );
    let input = match prepared.provider_projection(43144).unwrap() {
        AdapterBridgeProviderProjection::Create(input) => input,
        other => panic!("expected create projection, got {other:?}"),
    };
    assert_eq!(input.agent_id, AgentId::Claude);
    assert_eq!(input.settings_config["model"], "gpt-5.4");
    assert_eq!(
        input.settings_config["env"]["ANTHROPIC_BASE_URL"],
        "http://127.0.0.1:43144"
    );
    assert_eq!(input.settings_config["env"]["ANTHROPIC_MODEL"], "gpt-5.4");
    assert!(input.settings_config["env"]
        .get("CLAUDE_CODE_MAX_CONTEXT_TOKENS")
        .is_none());
    assert!(input.settings_config["env"]["ANTHROPIC_AUTH_TOKEN"]
        .as_str()
        .is_some_and(|token| token.starts_with("ahb_")));
    assert!(!serde_json::to_string(&input)
        .unwrap()
        .contains("codex-upstream-access-secret"));
    assert!(!serde_json::to_string(&input)
        .unwrap()
        .contains("refresh-must-not-enter-bridge"));
}

#[test]
fn prepare_codex_subscription_projects_chat_loopback_for_grok_kimi_dsh() {
    for (target, field, expected) in [
        (AgentId::Grok, "content", "http://127.0.0.1:43145/v1"),
        (AgentId::Kimi, "content", "http://127.0.0.1:43145/v1"),
        (AgentId::Dsh, "baseURL", "http://127.0.0.1:43145"),
    ] {
        let (_dir, db) = test_db();
        AccountRepo::new(db.clone())
            .create(&codex_subscription_account(
                "codex-subscription",
                "codex-upstream-access-secret",
            ))
            .unwrap();
        let service = AdapterBridgeService::new(db);
        let prepared = service
            .prepare(&codex_chat_request("codex-subscription", target))
            .unwrap();
        assert_eq!(prepared.profile().target_agent_id, target);
        assert_eq!(prepared.profile().route, AdapterRoute::LocalBridge);
        assert_eq!(
            prepared
                .runtime_material()
                .start_spec(None)
                .upstream
                .protocol,
            BridgeUpstreamProtocol::CodexResponsesOauth
        );
        assert_eq!(
            prepared
                .runtime_material()
                .start_spec(None)
                .upstream
                .model
                .as_deref(),
            Some("")
        );
        let input = match prepared.provider_projection(43145).unwrap() {
            AdapterBridgeProviderProjection::Create(input) => input,
            other => panic!("expected create projection, got {other:?}"),
        };
        assert_eq!(input.agent_id, target);
        let haystack = if field == "content" {
            input.settings_config["content"].as_str().unwrap_or("")
        } else {
            input.settings_config[field].as_str().unwrap_or("")
        };
        assert!(
            haystack.contains(expected),
            "{target:?} missing {expected} in {field}: {haystack}"
        );
        if target == AgentId::Grok {
            assert!(
                haystack.contains("api_backend = \"responses\""),
                "Codex→Grok projection must write api_backend=responses: {haystack}"
            );
            assert_eq!(
                prepared
                    .runtime_material()
                    .start_spec(None)
                    .upstream
                    .local_surface,
                BridgeLocalSurface::Responses
            );
        }
        assert!(!haystack.contains("grok-"), "{target:?} leftover grok-*");
        assert!(
            !haystack.contains("gpt-"),
            "{target:?} invented ChatGPT model"
        );
        assert!(!serde_json::to_string(&input)
            .unwrap()
            .contains("codex-upstream-access-secret"));
    }
}

#[test]
fn prepare_codex_subscription_requires_access_token() {
    let (_dir, db) = test_db();
    AccountRepo::new(db.clone())
        .create(&codex_subscription_account("codex-refresh-only", ""))
        .unwrap();
    let service = AdapterBridgeService::new(db);
    let error = service
        .prepare(&codex_claude_request("codex-refresh-only"))
        .unwrap_err();
    assert_eq!(error.code(), "invalid_arg");
    assert!(!format!("{error}").contains("refresh-must-not-enter-bridge"));
}

#[test]
fn prepare_anthropic_provider_projects_messages_bridge_not_kimi() {
    let (_dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&anthropic_source("anthropic-key", "sk-ant-secret"))
        .unwrap();
    let service = AdapterBridgeService::new(db.clone());

    let prepared = service
        .prepare(&anthropic_request(
            AdapterSourceKind::Provider,
            "anthropic-key",
        ))
        .unwrap();
    assert_eq!(prepared.profile().rule_id, ANTHROPIC_RULE_ID);
    assert_eq!(prepared.profile().source_kind, AdapterSourceKind::Provider);
    let start = prepared.runtime_material().start_spec(None);
    assert_eq!(start.upstream.base_url, ANTHROPIC_MESSAGES_BASE_URL);
    assert_eq!(
        start.upstream.protocol,
        BridgeUpstreamProtocol::AnthropicMessages
    );
    assert_eq!(
        start.upstream.model.as_deref(),
        Some(ANTHROPIC_DEFAULT_MODEL)
    );
    assert!(!format!("{prepared:?}").contains("sk-ant-secret"));

    let generated = create_projection(&db, &prepared, 43131);
    assert_eq!(generated.meta["adapterRuleId"], ANTHROPIC_RULE_ID);
    assert_eq!(
        generated.meta["adapterBridge"]["kind"],
        "responses_to_anthropic_messages"
    );
    assert_eq!(generated.meta["adapterSourceRef"]["kind"], "provider");
    let content = generated.settings_config["content"].as_str().unwrap();
    assert!(content.contains(ANTHROPIC_PROVIDER_SLUG));
    assert!(content.contains("AgentHub Anthropic Bridge"));
    assert!(content.contains(ANTHROPIC_DEFAULT_MODEL));
    assert!(!content.contains(PROVIDER_SLUG));
    assert!(!content.contains("kimi-k2.5"));
    assert!(!serde_json::to_string(&generated)
        .unwrap()
        .contains("sk-ant-secret"));
}

#[test]
fn prepare_anthropic_account_reuses_secret_resolver_and_projects_account_ref() {
    let (_dir, db) = test_db();
    AccountRepo::new(db.clone())
        .create(&anthropic_account("anthropic-account", "sk-ant-account"))
        .unwrap();
    let service = AdapterBridgeService::new(db.clone());

    let prepared = service
        .prepare(&anthropic_request(
            AdapterSourceKind::Account,
            "anthropic-account",
        ))
        .unwrap();
    assert_eq!(prepared.profile().rule_id, ANTHROPIC_RULE_ID);
    assert_eq!(prepared.profile().source_kind, AdapterSourceKind::Account);
    assert_eq!(
        prepared
            .runtime_material()
            .start_spec(None)
            .upstream
            .protocol,
        BridgeUpstreamProtocol::AnthropicMessages
    );
    assert!(!format!("{prepared:?}").contains("sk-ant-account"));

    let generated = create_projection(&db, &prepared, 43132);
    assert_eq!(generated.meta["adapterSourceRef"]["kind"], "account");
    assert_eq!(
        generated.meta["adapterSourceRef"]["id"],
        "anthropic-account"
    );
    assert_eq!(generated.meta["adapterRuleId"], ANTHROPIC_RULE_ID);
    service.finalize(&prepared, 43132).unwrap();
    let restored = service
        .resolve_restore_material(prepared.profile().id.as_str())
        .unwrap();
    assert_eq!(
        restored
            .runtime_material()
            .start_spec(None)
            .upstream
            .protocol,
        BridgeUpstreamProtocol::AnthropicMessages
    );
    assert!(!format!("{restored:?}").contains("sk-ant-account"));
}

#[test]
fn prepare_openai_provider_projects_chat_completions_bridge() {
    let (_dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&openai_source("openai-key", "sk-openai-secret"))
        .unwrap();
    let service = AdapterBridgeService::new(db.clone());

    let prepared = service
        .prepare(&openai_request(AdapterSourceKind::Provider, "openai-key"))
        .unwrap();
    assert_eq!(prepared.profile().rule_id, OPENAI_RULE_ID);
    assert_eq!(prepared.profile().source_kind, AdapterSourceKind::Provider);
    let start = prepared.runtime_material().start_spec(None);
    assert_eq!(start.upstream.base_url, OPENAI_CHAT_BASE_URL);
    assert_eq!(
        start.upstream.protocol,
        BridgeUpstreamProtocol::OpenAiChatCompletions
    );
    assert_eq!(start.upstream.model.as_deref(), Some(OPENAI_DEFAULT_MODEL));
    assert!(!format!("{prepared:?}").contains("sk-openai-secret"));

    let generated = create_projection(&db, &prepared, 43133);
    assert_eq!(generated.meta["adapterRuleId"], OPENAI_RULE_ID);
    assert_eq!(
        generated.meta["adapterBridge"]["kind"],
        "responses_to_chat_completions"
    );
    assert_eq!(generated.meta["adapterSourceRef"]["kind"], "provider");
    let content = generated.settings_config["content"].as_str().unwrap();
    assert!(content.contains(OPENAI_PROVIDER_SLUG));
    assert!(content.contains("AgentHub OpenAI Bridge"));
    assert!(content.contains(OPENAI_DEFAULT_MODEL));
    assert!(!content.contains(PROVIDER_SLUG));
    assert!(!content.contains("kimi-k2.5"));
    assert!(!content.contains("grok-"));
    assert!(!serde_json::to_string(&generated)
        .unwrap()
        .contains("sk-openai-secret"));
}

#[test]
fn prepare_openai_account_reuses_secret_resolver_and_projects_account_ref() {
    let (_dir, db) = test_db();
    AccountRepo::new(db.clone())
        .create(&openai_account("openai-account", "sk-openai-account"))
        .unwrap();
    let service = AdapterBridgeService::new(db.clone());

    let prepared = service
        .prepare(&openai_request(
            AdapterSourceKind::Account,
            "openai-account",
        ))
        .unwrap();
    assert_eq!(prepared.profile().rule_id, OPENAI_RULE_ID);
    assert_eq!(prepared.profile().source_kind, AdapterSourceKind::Account);
    assert_eq!(
        prepared
            .runtime_material()
            .start_spec(None)
            .upstream
            .protocol,
        BridgeUpstreamProtocol::OpenAiChatCompletions
    );
    assert!(!format!("{prepared:?}").contains("sk-openai-account"));

    let generated = create_projection(&db, &prepared, 43134);
    assert_eq!(generated.meta["adapterSourceRef"]["kind"], "account");
    assert_eq!(generated.meta["adapterSourceRef"]["id"], "openai-account");
    assert_eq!(generated.meta["adapterRuleId"], OPENAI_RULE_ID);
    service.finalize(&prepared, 43134).unwrap();
    let restored = service
        .resolve_restore_material(prepared.profile().id.as_str())
        .unwrap();
    assert_eq!(
        restored
            .runtime_material()
            .start_spec(None)
            .upstream
            .protocol,
        BridgeUpstreamProtocol::OpenAiChatCompletions
    );
    assert_eq!(
        restored
            .runtime_material()
            .start_spec(None)
            .upstream
            .base_url,
        OPENAI_CHAT_BASE_URL
    );
    assert!(!format!("{restored:?}").contains("sk-openai-account"));
}

#[test]
fn prepare_openai_account_uses_its_saved_endpoint_and_model() {
    let (_dir, db) = test_db();
    let mut account = openai_account("workbuddy-account", "sk-workbuddy-account");
    account.label = "Custom Grok".into();
    account.credentials = json!({
        "format": "api_key",
        "api_key": "sk-workbuddy-account",
        "base_url": "https://relay.example/custom/v1/chat/completions",
        "model_id": "grok-4.6",
    });
    account.extra = json!({"provider": "openai"});
    AccountRepo::new(db.clone()).create(&account).unwrap();
    let service = AdapterBridgeService::new(db.clone());

    let prepared = service
        .prepare(&openai_request(
            AdapterSourceKind::Account,
            "workbuddy-account",
        ))
        .unwrap();
    let start = prepared.runtime_material().start_spec(None);
    assert_eq!(start.upstream.base_url, "https://relay.example/custom/v1");
    assert_eq!(start.upstream.model.as_deref(), Some("grok-4.6"));
    assert!(!format!("{prepared:?}").contains("sk-workbuddy-account"));

    create_projection(&db, &prepared, 43135);
    service.finalize(&prepared, 43135).unwrap();
    let restored = service
        .resolve_restore_material(prepared.profile().id.as_str())
        .unwrap();
    let restored_start = restored.runtime_material().start_spec(None);
    assert_eq!(
        restored_start.upstream.base_url,
        "https://relay.example/custom/v1"
    );
    assert_eq!(restored_start.upstream.model.as_deref(), Some("grok-4.6"));
    assert!(!format!("{restored:?}").contains("sk-workbuddy-account"));
}

#[test]
fn workbuddy_account_upstream_uses_its_saved_url_and_model_id() {
    let (_dir, db) = test_db();
    let mut account = openai_account("workbuddy-catalog", "sk-workbuddy-catalog");
    account.agent_id = AgentId::WorkBuddy;
    account.label = "WorkBuddy Grok".into();
    account.credentials = json!({
        "format": "api_key",
        "api_key": "sk-workbuddy-catalog",
        "url": "https://relay.example/custom/v1/chat/completions",
        "id": "grok-4.6",
    });
    account.extra = json!({"provider": "workbuddy"});
    AccountRepo::new(db.clone()).create(&account).unwrap();
    let service = AdapterBridgeService::new(db);

    let (url, model, ..) = super::prepare::openai_source_upstream(
        &service,
        &OPENAI_CODEX_RULE,
        AdapterSourceKind::Account,
        "workbuddy-catalog",
    );
    assert_eq!(url, "https://relay.example/custom/v1");
    assert_eq!(model, "grok-4.6");
}

#[test]
fn default_pool_routes_workbuddy_deepseek_to_real_upstream_for_all_supported_agents() {
    for (target, surface, local_surface) in [
        (
            AgentId::Kimi,
            RouteDownstreamSurface::ChatCompletions,
            BridgeLocalSurface::ChatCompletions,
        ),
        (
            AgentId::Dsh,
            RouteDownstreamSurface::ChatCompletions,
            BridgeLocalSurface::ChatCompletions,
        ),
        (
            AgentId::Claude,
            RouteDownstreamSurface::Messages,
            BridgeLocalSurface::Messages,
        ),
        (
            AgentId::Codex,
            RouteDownstreamSurface::Responses,
            BridgeLocalSurface::Responses,
        ),
        (
            AgentId::Grok,
            RouteDownstreamSurface::Responses,
            BridgeLocalSurface::Responses,
        ),
    ] {
        let (_dir, db) = test_db();
        db.set_setting(FEATURE_ROUTE_POOL_V2, "true").unwrap();
        let mut account = openai_account("workbuddy-deepseek", "sk-deepseek");
        account.agent_id = AgentId::WorkBuddy;
        account.label = "DeepSeek-V4 Flash".into();
        account.credentials = json!({
            "format": "api_key",
            "api_key": "sk-deepseek",
            "endpoint": "https://api.deepseek.com/v1/chat/completions",
            "model_id": "deepseek-v4-flash",
        });
        account.extra = json!({
            "provider": "workbuddy",
            "surface": "deepseek-api",
            "model_id": "deepseek-v4-flash",
        });
        AccountRepo::new(db.clone()).create(&account).unwrap();

        let pools = RoutePoolService::new(db.clone());
        let pool = pools.ensure_default_pool(target, surface).unwrap();
        pools
            .add_member(&pool.id, AdapterSourceKind::Account, &account.id)
            .unwrap();

        let spec = AdapterBridgeService::new(db).pool_listener_spec(&pool, (false, false));
        assert_eq!(spec.upstream.base_url, "https://api.deepseek.com/v1");
        assert_eq!(spec.upstream.model.as_deref(), Some("deepseek-v4-flash"));
        assert_eq!(spec.upstream.local_surface, local_surface);
        assert_eq!(
            spec.upstream.protocol,
            BridgeUpstreamProtocol::OpenAiChatCompletions
        );
        assert!(spec.upstream.auth.has_token());
        assert_ne!(spec.upstream.base_url, "http://127.0.0.1/");
        assert_eq!(spec.members.len(), 1);
    }
}

#[test]
fn default_pool_resolves_provider_backed_openai_compatible_keys() {
    for (preset, endpoint, model) in [
        ("deepseek-api", "https://api.deepseek.com/v1", "deepseek-chat"),
        (
            "glm-coding-plan",
            "https://open.bigmodel.cn/api/coding/paas/v4",
            "glm-4.6",
        ),
        ("xai-api", "https://api.x.ai/v1", "grok-4"),
    ] {
        for (target, surface) in [
            (AgentId::Kimi, RouteDownstreamSurface::ChatCompletions),
            (AgentId::Dsh, RouteDownstreamSurface::ChatCompletions),
            (AgentId::Claude, RouteDownstreamSurface::Messages),
            (AgentId::Codex, RouteDownstreamSurface::Responses),
            (AgentId::Grok, RouteDownstreamSurface::Responses),
        ] {
            let (_dir, db) = test_db();
            let source_id = format!("{preset}-{target}");
            ProviderRepo::new(db.clone())
                .create(&Provider {
                    id: source_id.clone(),
                    agent_id: AgentId::WorkBuddy,
                    name: format!("{preset} provider"),
                    settings_config: json!({
                        "api_key": format!("sk-{preset}"),
                        "base_url": endpoint,
                        "model": model,
                    }),
                    meta: json!({"preset": preset}),
                    is_current: false,
                    created_at: "now".into(),
                    updated_at: "now".into(),
                })
                .unwrap();

            let pools = RoutePoolService::new(db.clone());
            let pool = pools.ensure_default_pool(target, surface).unwrap();
            pools
                .add_member(&pool.id, AdapterSourceKind::Provider, &source_id)
                .unwrap();

            let spec = AdapterBridgeService::new(db).pool_listener_spec(&pool, (false, false));
            assert_eq!(spec.upstream.base_url, endpoint, "{preset} -> {target}");
            assert_eq!(spec.upstream.model.as_deref(), Some(model));
            assert_eq!(spec.upstream.auth.token(), format!("sk-{preset}"));
            assert_ne!(spec.upstream.auth.token(), "pending");
        }
    }
}

#[test]
fn legacy_pool_does_not_mix_login_keys_across_upstream_endpoints() {
    let (_dir, db) = test_db();
    let mut lead = openai_account("workbuddy-qooo", "sk-qooo");
    lead.agent_id = AgentId::WorkBuddy;
    lead.label = "WorkBuddy Grok".into();
    lead.credentials = json!({
        "format": "api_key",
        "api_key": "sk-qooo",
        "url": "https://qooo.example/v1/chat/completions",
        "base_url": "https://qooo.example/v1/chat/completions",
        "id": "grok-4.6",
    });
    lead.extra = json!({"provider": "workbuddy"});
    let mut other = lead.clone();
    other.id = "workbuddy-other".into();
    other.label = "Other relay".into();
    other.credentials["api_key"] = json!("sk-other");
    other.credentials["url"] = json!("https://other.example/v1/chat/completions");
    other.credentials["base_url"] = json!("https://other.example/v1/chat/completions");
    AccountRepo::new(db.clone()).create(&lead).unwrap();
    AccountRepo::new(db.clone()).create(&other).unwrap();

    let pools = RoutePoolService::new(db.clone());
    let pool = pools
        .ensure_default_pool(AgentId::Kimi, RouteDownstreamSurface::ChatCompletions)
        .unwrap();
    pools
        .add_member(&pool.id, AdapterSourceKind::Account, "workbuddy-qooo")
        .unwrap();
    pools
        .add_member(&pool.id, AdapterSourceKind::Account, "workbuddy-other")
        .unwrap();

    let spec = AdapterBridgeService::new(db).pool_listener_spec(&pool, (false, false));
    assert_eq!(spec.upstream.base_url, "https://qooo.example/v1");
    assert_eq!(spec.members.len(), 1, "different endpoint key must not enter v1 picker");
    assert_eq!(spec.members[0].label, "WorkBuddy Grok");
    assert_eq!(spec.members[0].auth.token(), "sk-qooo");
}

#[test]
fn legacy_grok_claude_kind_is_migratable_on_prepare() {
    let (_dir, db) = test_db();
    AccountRepo::new(db.clone())
        .create(&grok_subscription_account(
            "grok-subscription",
            "grok-upstream-secret",
        ))
        .unwrap();
    let service = AdapterBridgeService::new(db.clone());
    let prepared = service
        .prepare(&grok_claude_account_request("grok-subscription"))
        .unwrap();
    let generated = create_projection(&db, &prepared, 43123);
    let mut legacy = generated.clone();
    legacy.meta["adapterBridge"]["kind"] = json!("messages_to_xai_chat_completions");
    persist_mutated_provider(&db, legacy);

    let retried = service
        .prepare(&grok_claude_account_request("grok-subscription"))
        .unwrap();
    let input = match retried.provider_projection(43123).unwrap() {
        AdapterBridgeProviderProjection::Update(input) => input,
        other => panic!("expected update projection, got {other:?}"),
    };
    assert_eq!(
        input.meta["adapterBridge"]["kind"],
        "messages_to_xai_responses"
    );
    assert_eq!(input.meta["adapterRuleVersion"], 1);
}

#[test]
fn legacy_grok_codex_kind_forces_meta_rewrite() {
    let (_dir, db) = test_db();
    AccountRepo::new(db.clone())
        .create(&grok_subscription_account(
            "grok-subscription",
            "grok-upstream-secret",
        ))
        .unwrap();
    let service = AdapterBridgeService::new(db.clone());
    let prepared = service
        .prepare(&grok_codex_account_request("grok-subscription"))
        .unwrap();
    let generated = create_projection(&db, &prepared, 43123);
    service.finalize(&prepared, 43123).unwrap();
    let mut legacy = generated.clone();
    legacy.meta["adapterBridge"]["kind"] = json!("responses_to_chat_completions");
    persist_mutated_provider(&db, legacy);

    let retried = service
        .prepare(&grok_codex_account_request("grok-subscription"))
        .unwrap();
    assert_eq!(retried.profile().status, AdapterProfileStatus::Active);
    assert_eq!(retried.profile().local_port, Some(43123));
    let input = match retried.provider_projection(43123).unwrap() {
        AdapterBridgeProviderProjection::Update(input) => input,
        other => panic!("legacy kind must force Update even when Active+port match: {other:?}"),
    };
    assert_eq!(
        input.meta["adapterBridge"]["kind"],
        "responses_to_xai_responses"
    );
}

#[test]
fn legacy_codex_grok_chat_completions_toml_rewrites_to_responses() {
    let (_dir, db) = test_db();
    AccountRepo::new(db.clone())
        .create(&codex_subscription_account(
            "codex-subscription",
            "codex-upstream-access-secret",
        ))
        .unwrap();
    let service = AdapterBridgeService::new(db.clone());
    let prepared = service
        .prepare(&codex_chat_request("codex-subscription", AgentId::Grok))
        .unwrap();
    let generated = create_projection(&db, &prepared, 43145);
    service.finalize(&prepared, 43145).unwrap();
    let mut legacy = generated.clone();
    let content = legacy.settings_config["content"].as_str().unwrap();
    legacy.settings_config["content"] = json!(content.replace(
        "api_backend = \"responses\"",
        "api_backend = \"chat_completions\""
    ));
    persist_mutated_provider(&db, legacy);

    let retried = service
        .prepare(&codex_chat_request("codex-subscription", AgentId::Grok))
        .unwrap();
    let input = match retried.provider_projection(43145).unwrap() {
        AdapterBridgeProviderProjection::Update(input) => input,
        other => panic!("expected update projection, got {other:?}"),
    };
    let content = input.settings_config["content"].as_str().unwrap();
    assert!(content.contains("api_backend = \"responses\""));
    assert!(!content.contains("api_backend = \"chat_completions\""));
}

#[test]
fn legacy_projection_preflight_remove_succeeds() {
    let (_dir, db) = test_db();
    AccountRepo::new(db.clone())
        .create(&grok_subscription_account(
            "grok-subscription",
            "grok-upstream-secret",
        ))
        .unwrap();
    let service = AdapterBridgeService::new(db.clone());
    let prepared = service
        .prepare(&grok_claude_account_request("grok-subscription"))
        .unwrap();
    let generated = create_projection(&db, &prepared, 43123);
    let profile = service.finalize(&prepared, 43123).unwrap();
    let mut legacy = generated.clone();
    legacy.meta["adapterBridge"]["kind"] = json!("messages_to_xai_chat_completions");
    persist_mutated_provider(&db, legacy);

    let removal = service.preflight_remove(&profile.id).unwrap();
    assert_eq!(removal.profile().id, profile.id);
}

#[test]
fn legacy_projection_restore_flags_reprojection() {
    let (_dir, db) = test_db();
    AccountRepo::new(db.clone())
        .create(&grok_subscription_account(
            "grok-subscription",
            "grok-upstream-secret",
        ))
        .unwrap();
    let service = AdapterBridgeService::new(db.clone());
    let prepared = service
        .prepare(&grok_codex_account_request("grok-subscription"))
        .unwrap();
    let generated = create_projection(&db, &prepared, 43123);
    let profile = service.finalize(&prepared, 43123).unwrap();
    let mut legacy = generated.clone();
    legacy.meta["adapterBridge"]["kind"] = json!("responses_to_chat_completions");
    persist_mutated_provider(&db, legacy);

    let restored = service.resolve_restore_material(&profile.id).unwrap();
    assert!(restored.needs_reprojection());
}

#[test]
fn unknown_bridge_kind_fails_closed() {
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
    let mut unknown = generated.clone();
    unknown.meta["adapterBridge"]["kind"] = json!("not_a_supported_kind");
    persist_mutated_provider(&db, unknown);
    assert_eq!(
        service
            .prepare(&request("kimi-membership"))
            .unwrap_err()
            .code(),
        "adapter.provider_conflict"
    );
}

#[test]
fn legacy_toml_with_drifted_port_still_conflicts() {
    let (_dir, db) = test_db();
    AccountRepo::new(db.clone())
        .create(&codex_subscription_account(
            "codex-subscription",
            "codex-upstream-access-secret",
        ))
        .unwrap();
    let service = AdapterBridgeService::new(db.clone());
    let prepared = service
        .prepare(&codex_chat_request("codex-subscription", AgentId::Grok))
        .unwrap();
    let generated = create_projection(&db, &prepared, 43145);
    service.finalize(&prepared, 43145).unwrap();
    let mut drifted = generated.clone();
    let content = drifted.settings_config["content"].as_str().unwrap();
    drifted.settings_config["content"] = json!(content
        .replace(
            "api_backend = \"responses\"",
            "api_backend = \"chat_completions\""
        )
        .replace("43145", "43199"));
    persist_mutated_provider(&db, drifted);
    assert_eq!(
        service
            .prepare(&codex_chat_request("codex-subscription", AgentId::Grok))
            .unwrap_err()
            .code(),
        "adapter.provider_conflict"
    );
}

#[test]
fn start_spec_does_not_inject_retired_openrouter_backup() {
    let material = AdapterBridgeRuntimeMaterial {
        profile_id: "openrouter-codex-models".into(),
        source_id: "openrouter".into(),
        preferred_port: None,
        upstream_base_url: "https://openrouter.ai/api/v1".into(),
        upstream_model: OPENAI_DEFAULT_MODEL.into(),
        configured_listed_models: Vec::new(),
        context_window_tokens: None,
        protocol: BridgeUpstreamProtocol::OpenAiChatCompletions,
        local_surface: BridgeLocalSurface::Responses,
        source: AdapterSourceProduct::OpenaiApi,
        target_agent: AgentId::Codex,
        downstream_dialect: RouteDownstreamDialect::Generic,
        upstream_auth: ResolvedAuth::bearer("sk-or-placeholder-test-key"),
        local_bearer: "local-secret".into(),
        route_index: None,
        index_enabled: false,
        codex_ingress_grok_upstream: false,
        grok_ingress_codex_upstream: false,
        schedule_policy: Default::default(),
    };
    let listed = material.start_spec(Some(0)).listed_models;
    assert!(
        listed.is_empty(),
        "empty user list follows downstream: {listed:?}"
    );
}

#[test]
fn start_spec_keeps_every_user_listed_model() {
    let material = AdapterBridgeRuntimeMaterial {
        profile_id: "openrouter-listed".into(),
        source_id: "openrouter".into(),
        preferred_port: None,
        upstream_base_url: "https://openrouter.ai/api/v1".into(),
        upstream_model: "openai/gpt-4o".into(),
        configured_listed_models: vec!["openai/gpt-4o".into(), "anthropic/claude-sonnet-4".into()],
        context_window_tokens: None,
        protocol: BridgeUpstreamProtocol::OpenAiChatCompletions,
        local_surface: BridgeLocalSurface::Responses,
        source: AdapterSourceProduct::OpenaiApi,
        target_agent: AgentId::Codex,
        downstream_dialect: RouteDownstreamDialect::Generic,
        upstream_auth: ResolvedAuth::bearer("sk-or-placeholder-test-key"),
        local_bearer: "local-secret".into(),
        route_index: None,
        index_enabled: false,
        codex_ingress_grok_upstream: false,
        grok_ingress_codex_upstream: false,
        schedule_policy: Default::default(),
    };
    let listed = material.start_spec(Some(0)).listed_models;
    assert!(listed.iter().any(|model| model == "openai/gpt-4o"));
    assert!(listed
        .iter()
        .any(|model| model == "anthropic/claude-sonnet-4"));
    assert!(!listed.iter().any(|model| model == "stealth/ox-alpha"));
}

#[test]
fn start_spec_strips_claude_1m_marker_from_listed_models() {
    let material = AdapterBridgeRuntimeMaterial {
        profile_id: "openrouter-1m".into(),
        source_id: "openrouter".into(),
        preferred_port: None,
        upstream_base_url: "https://openrouter.ai/api/v1".into(),
        upstream_model: "stealth/ox-alpha".into(),
        configured_listed_models: vec!["stealth/ox-alpha[1m]".into()],
        context_window_tokens: Some(1_048_576),
        protocol: BridgeUpstreamProtocol::OpenAiChatCompletions,
        local_surface: BridgeLocalSurface::Responses,
        source: AdapterSourceProduct::OpenaiApi,
        target_agent: AgentId::Claude,
        downstream_dialect: RouteDownstreamDialect::Generic,
        upstream_auth: ResolvedAuth::bearer("sk-or-placeholder-test-key"),
        local_bearer: "local-secret".into(),
        route_index: None,
        index_enabled: false,
        codex_ingress_grok_upstream: false,
        grok_ingress_codex_upstream: false,
        schedule_policy: Default::default(),
    };
    let listed = material.start_spec(Some(0)).listed_models;
    assert!(!listed.iter().any(|model| model == "stealth/ox-alpha"));
    assert!(!listed.iter().any(|model| model.contains('[')));
}

#[test]
fn start_spec_official_openai_does_not_list_stealth() {
    let material = AdapterBridgeRuntimeMaterial {
        profile_id: "openai-official".into(),
        source_id: "openai-api".into(),
        preferred_port: None,
        upstream_base_url: OPENAI_CHAT_BASE_URL.into(),
        upstream_model: OPENAI_DEFAULT_MODEL.into(),
        configured_listed_models: Vec::new(),
        context_window_tokens: None,
        protocol: BridgeUpstreamProtocol::OpenAiChatCompletions,
        local_surface: BridgeLocalSurface::Responses,
        source: AdapterSourceProduct::OpenaiApi,
        target_agent: AgentId::Codex,
        downstream_dialect: RouteDownstreamDialect::Codex,
        upstream_auth: ResolvedAuth::bearer("openai-upstream-secret"),
        local_bearer: "local-secret".into(),
        route_index: None,
        index_enabled: false,
        codex_ingress_grok_upstream: false,
        grok_ingress_codex_upstream: false,
        schedule_policy: Default::default(),
    };
    let listed = material.start_spec(Some(0)).listed_models;
    assert!(!listed.iter().any(|model| model == "stealth/ox-alpha"));
}

#[test]
fn prepare_glm_claude_is_native_not_local_bridge() {
    let (_dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&Provider {
            id: "glm-create".into(),
            agent_id: AgentId::Claude,
            name: "Zhipu".into(),
            settings_config: json!({
                "apiKey": "glm-secret",
                "baseURL": "https://open.bigmodel.cn/api/coding/paas/v4",
                "vendor": "zhipu",
                "listedModels": ["glm-4.6"],
                "endpoints": [
                    {"target": "claude", "enabled": true, "url": "https://open.bigmodel.cn/api/anthropic"},
                    {"target": "codex", "enabled": true, "url": "https://open.bigmodel.cn/api/coding/paas/v4"}
                ]
            }),
            meta: json!({"preset": "openai-compat"}),
            is_current: false,
            created_at: "now".into(),
            updated_at: "now".into(),
        })
        .unwrap();
    let error = AdapterBridgeService::new(db.clone())
        .prepare(&AdapterBridgePrepareRequest {
            source_kind: AdapterSourceKind::Provider,
            source_id: "glm-create".into(),
            target_agent_id: AgentId::Claude,
            auto_start: true,
        })
        .unwrap_err();
    assert_eq!(error.code(), "unsupported");
}

#[test]
fn prepare_openrouter_claude_skips_retired_backup_and_keeps_1m_window() {
    let (_dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&Provider {
            id: "or-create".into(),
            agent_id: AgentId::Claude,
            name: "OpenRouter".into(),
            settings_config: json!({
                "apiKey": "sk-or-test",
                "baseURL": "https://openrouter.ai/api/v1",
                "vendor": "openrouter",
                "listedModels": ["stealth/ox-alpha", "anthropic/claude-sonnet-4"],
                "model": "stealth/ox-alpha",
                "contextWindowTokens": 1_048_576
            }),
            meta: json!({"preset": "openrouter"}),
            is_current: false,
            created_at: "now".into(),
            updated_at: "now".into(),
        })
        .unwrap();
    let service = AdapterBridgeService::new(db.clone());
    let prepared = service
        .prepare(&AdapterBridgePrepareRequest {
            source_kind: AdapterSourceKind::Provider,
            source_id: "or-create".into(),
            target_agent_id: AgentId::Claude,
            auto_start: true,
        })
        .unwrap();
    let input = match prepared.provider_projection(43151).unwrap() {
        AdapterBridgeProviderProjection::Create(input) => input,
        other => panic!("expected create projection, got {other:?}"),
    };
    assert_eq!(input.settings_config["model"], "anthropic/claude-sonnet-4");
    assert_eq!(
        input.settings_config["env"]["ANTHROPIC_MODEL"],
        "anthropic/claude-sonnet-4"
    );
    assert_eq!(
        input.settings_config["env"]["CLAUDE_CODE_MAX_CONTEXT_TOKENS"],
        "1048576"
    );
    assert_eq!(
        input.settings_config["env"]["CLAUDE_CODE_AUTO_COMPACT_WINDOW"],
        "1048576"
    );
}

#[test]
fn prepare_deepseek_claude_is_native_not_local_bridge() {
    let (_dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&Provider {
            id: "ds-create".into(),
            agent_id: AgentId::Claude,
            name: "DeepSeek".into(),
            settings_config: json!({
                "apiKey": "ds-secret",
                "baseURL": "https://api.deepseek.com",
                "vendor": "deepseek",
                "endpoints": [
                    {"target": "claude", "enabled": true, "url": "https://api.deepseek.com/anthropic"}
                ]
            }),
            meta: json!({"preset": "openai-compat"}),
            is_current: false,
            created_at: "now".into(),
            updated_at: "now".into(),
        })
        .unwrap();
    let error = AdapterBridgeService::new(db)
        .prepare(&AdapterBridgePrepareRequest {
            source_kind: AdapterSourceKind::Provider,
            source_id: "ds-create".into(),
            target_agent_id: AgentId::Claude,
            auto_start: true,
        })
        .unwrap_err();
    assert_eq!(error.code(), "unsupported");
}

#[test]
fn prepare_openai_toml_uses_the_active_model_provider_base_url() {
    let (_dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&Provider {
            id: "openai-toml-active".into(),
            agent_id: AgentId::Codex,
            name: "Codex custom relay".into(),
            settings_config: json!({
                "apiKey": "sk-openai-toml-active",
                "format": "toml",
                "content": "model_provider = \"active\"\nmodel = \"relay-model\"\n\n[model_providers.inactive]\nbase_url = \"https://inactive.example/v1\"\n\n[model_providers.active]\nbase_url = \"https://active.example/v1\"\n"
            }),
            meta: json!({"preset": "openai-compatible"}),
            is_current: false,
            created_at: "now".into(),
            updated_at: "now".into(),
        })
        .unwrap();

    let prepared = AdapterBridgeService::new(db)
        .prepare(&openai_request(
            AdapterSourceKind::Provider,
            "openai-toml-active",
        ))
        .unwrap();
    let start = prepared.runtime_material().start_spec(None);
    assert_eq!(start.upstream.base_url, "https://active.example/v1");
    assert_eq!(start.upstream.model.as_deref(), Some("relay-model"));
}

#[test]
fn prepare_rejects_openai_marker_when_active_toml_host_is_not_official() {
    let (_dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&Provider {
            id: "openai-toml-bad-host".into(),
            agent_id: AgentId::Codex,
            name: "Mislabelled OpenAI".into(),
            settings_config: json!({
                "apiKey": "sk-openai-toml-bad-host",
                "format": "toml",
                "content": "model_provider = \"active\"\n\n[model_providers.active]\nbase_url = \"https://api.openai.com.evil.example/v1\"\n"
            }),
            meta: json!({"preset": "openai"}),
            is_current: false,
            created_at: "now".into(),
            updated_at: "now".into(),
        })
        .unwrap();

    assert_eq!(
        AdapterBridgeService::new(db)
            .prepare(&openai_request(
                AdapterSourceKind::Provider,
                "openai-toml-bad-host",
            ))
            .unwrap_err()
            .code(),
        "unsupported"
    );
}

#[test]
fn production_start_spec_attaches_index_when_flags_on_and_pool_enrolled() {
    let (_dir, db) = test_db();
    db.set_setting(FEATURE_ROUTE_POOL_V2, "true").unwrap();
    db.set_setting(FEATURE_ROUTE_INDEX_V2, "true").unwrap();
    ProviderRepo::new(db.clone())
        .create(&kimi_source("kimi-membership", "upstream-secret"))
        .unwrap();
    let service = AdapterBridgeService::new(db.clone());
    let prepared = service.prepare(&request("kimi-membership")).unwrap();
    assert!(
        prepared
            .runtime_material()
            .start_spec(Some(0))
            .route_index
            .is_none(),
        "unenrolled pool must not attach an index"
    );
    RoutePoolService::new(db.clone())
        .enroll_unified_gateway(&prepared.profile().id, 43155)
        .unwrap();
    let prepared = service.prepare(&request("kimi-membership")).unwrap();
    let spec = prepared.runtime_material().start_spec(Some(0));
    let index = spec.route_index.expect("enrolled v2 start attaches index");
    assert!(!index.list_models("responses").is_empty());
    assert!(index.resolve("responses", "unknown-model").is_err());
    assert_eq!(
        spec.schedule_policy,
        RouteSchedulePolicy::PriorityFailover,
        "default pool policy remains priority_failover"
    );
}

#[test]
fn production_start_spec_attaches_pool_schedule_policy() {
    let (_dir, db) = test_db();
    db.set_setting(FEATURE_ROUTE_POOL_V2, "true").unwrap();
    db.set_setting(FEATURE_ROUTE_INDEX_V2, "true").unwrap();
    ProviderRepo::new(db.clone())
        .create(&kimi_source("kimi-membership", "upstream-secret"))
        .unwrap();
    let service = AdapterBridgeService::new(db.clone());
    let prepared = service.prepare(&request("kimi-membership")).unwrap();
    RoutePoolService::new(db.clone())
        .enroll_unified_gateway(&prepared.profile().id, 43155)
        .unwrap();
    let mut pool = RoutePoolService::new(db.clone())
        .get(&prepared.profile().id)
        .unwrap()
        .expect("legacy pool");
    pool.downstream_dialect = RouteDownstreamDialect::Grok;
    pool.schedule_policy = RouteSchedulePolicy::RoundRobin;
    RoutePoolRepo::new(db.clone())
        .update_pool(&pool)
        .expect("persist round_robin");
    let prepared = service.prepare(&request("kimi-membership")).unwrap();
    assert_eq!(
        prepared.runtime_material().preferred_port(),
        Some(43155),
        "persisted RoutePool gateway port is authoritative once enrolled"
    );
    assert_eq!(
        prepared
            .runtime_material()
            .start_spec(Some(0))
            .schedule_policy,
        RouteSchedulePolicy::RoundRobin
    );
    assert_eq!(
        prepared
            .runtime_material()
            .start_spec(Some(0))
            .downstream_responses_profile,
        Some(crate::bridge::DownstreamResponsesProfile::new(
            crate::bridge::ResponsesDialect::Grok,
        )),
        "persisted RoutePool dialect overrides the legacy target-agent fallback"
    );
}

#[test]
fn route_pool_disabled_keeps_legacy_target_agent_fallback() {
    let (_dir, db) = test_db();
    db.set_setting(FEATURE_ROUTE_POOL_V2, "off").unwrap();
    ProviderRepo::new(db.clone())
        .create(&kimi_source("kimi-membership", "upstream-secret"))
        .unwrap();

    let prepared = AdapterBridgeService::new(db)
        .prepare(&request("kimi-membership"))
        .unwrap();
    assert_eq!(
        prepared.runtime_material().downstream_dialect(),
        RouteDownstreamDialect::Codex,
        "disabled RoutePool keeps target-agent fallback"
    );
    assert_eq!(
        prepared
            .runtime_material()
            .start_spec(Some(0))
            .downstream_responses_profile,
        Some(crate::bridge::DownstreamResponsesProfile::new(
            crate::bridge::ResponsesDialect::Codex,
        ))
    );
}

#[test]
fn persisted_route_pool_surface_mismatch_fails_closed() {
    let (_dir, db) = test_db();
    db.set_setting(FEATURE_ROUTE_POOL_V2, "true").unwrap();
    ProviderRepo::new(db.clone())
        .create(&kimi_source("kimi-membership", "upstream-secret"))
        .unwrap();
    let service = AdapterBridgeService::new(db.clone());
    let prepared = service.prepare(&request("kimi-membership")).unwrap();
    let mut pool = RoutePoolService::new(db.clone())
        .get(&prepared.profile().id)
        .unwrap()
        .expect("legacy pool");
    pool.downstream_surface = RouteDownstreamSurface::Messages;
    RoutePoolRepo::new(db.clone()).update_pool(&pool).unwrap();

    let error = service
        .prepare(&request("kimi-membership"))
        .expect_err("surface mismatch must not start with an invalid profile");
    assert_eq!(error.code(), "adapter.route_pool_invalid");
    assert_eq!(
        error.to_string(),
        "The saved route format does not match this endpoint. Re-save the route and try again."
    );
}

#[test]
fn persisted_route_pool_dialect_mismatch_fails_closed() {
    let (_dir, db) = test_db();
    db.set_setting(FEATURE_ROUTE_POOL_V2, "true").unwrap();
    ProviderRepo::new(db.clone())
        .create(&kimi_source("kimi-membership", "upstream-secret"))
        .unwrap();
    let service = AdapterBridgeService::new(db.clone());
    let prepared = service.prepare(&request("kimi-membership")).unwrap();
    let mut pool = RoutePoolService::new(db.clone())
        .get(&prepared.profile().id)
        .unwrap()
        .expect("legacy pool");
    pool.downstream_dialect = RouteDownstreamDialect::Claude;
    RoutePoolRepo::new(db.clone()).update_pool(&pool).unwrap();

    let error = service
        .prepare(&request("kimi-membership"))
        .expect_err("dialect mismatch must not start with an invalid profile");
    assert_eq!(error.code(), "adapter.route_pool_invalid");
    assert_eq!(
        error.to_string(),
        "The saved route format does not match this endpoint. Re-save the route and try again."
    );
}

#[test]
fn persisted_route_pool_unparseable_dialect_is_not_silently_fallback() {
    let (_dir, db) = test_db();
    db.set_setting(FEATURE_ROUTE_POOL_V2, "true").unwrap();
    ProviderRepo::new(db.clone())
        .create(&kimi_source("kimi-membership", "upstream-secret"))
        .unwrap();
    let service = AdapterBridgeService::new(db.clone());
    let prepared = service.prepare(&request("kimi-membership")).unwrap();
    db.with_conn(|conn| {
        conn.execute_batch("PRAGMA ignore_check_constraints = ON")?;
        conn.execute(
            "UPDATE route_pools SET downstream_dialect = 'not-a-dialect' WHERE id = ?1",
            rusqlite::params![prepared.profile().id],
        )?;
        conn.execute_batch("PRAGMA ignore_check_constraints = OFF")?;
        Ok(())
    })
    .unwrap();

    let error = service
        .prepare(&request("kimi-membership"))
        .expect_err("invalid persisted dialect must propagate instead of fallback");
    assert_eq!(error.code(), "route_pool.invalid_data");
    assert!(error.to_string().contains("invalid downstream_dialect"));
}

#[test]
fn non_responses_surface_never_creates_downstream_responses_profile() {
    let mut material = AdapterBridgeRuntimeMaterial::for_test(
        "non-responses-profile",
        None,
        "local-secret",
        "upstream-secret",
    );
    material.local_surface = BridgeLocalSurface::Messages;
    material.downstream_dialect = RouteDownstreamDialect::Grok;
    assert_eq!(
        material.start_spec(Some(0)).downstream_responses_profile,
        None
    );
}

#[test]
fn production_start_spec_skips_index_when_route_index_flag_is_off() {
    let (_dir, db) = test_db();
    db.set_setting(FEATURE_ROUTE_POOL_V2, "true").unwrap();
    db.set_setting(FEATURE_ROUTE_INDEX_V2, "off").unwrap();
    ProviderRepo::new(db.clone())
        .create(&kimi_source("kimi-membership", "upstream-secret"))
        .unwrap();
    let service = AdapterBridgeService::new(db.clone());
    let prepared = service.prepare(&request("kimi-membership")).unwrap();
    RoutePoolService::new(db.clone())
        .enroll_unified_gateway(&prepared.profile().id, 43155)
        .unwrap();
    let prepared = service.prepare(&request("kimi-membership")).unwrap();
    assert!(
        prepared
            .runtime_material()
            .start_spec(Some(0))
            .route_index
            .is_none(),
        "flag off must keep v1 lead dispatch"
    );
}

#[test]
fn parse_openai_models_json_reads_data_ids() {
    let body = br#"{"data":[{"id":"gpt-4o"},{"id":" gpt-4.1 "},{"id":""}]}"#;
    assert_eq!(
        parse_openai_models_json(body).as_deref(),
        Some(["gpt-4o".to_owned(), "gpt-4.1".to_owned()].as_slice())
    );
    assert!(parse_openai_models_json(br#"{"data":[]}"#).is_none());
}

fn openai_source_with_listed(id: &str, api_key: &str, listed: &[&str]) -> Provider {
    Provider {
        id: id.into(),
        agent_id: AgentId::Codex,
        name: "OpenAI API".into(),
        settings_config: json!({
            "apiKey": api_key,
            "listedModels": listed,
        }),
        meta: json!({"preset": "openai"}),
        is_current: false,
        created_at: "now".into(),
        updated_at: "now".into(),
    }
}

#[test]
fn production_index_uses_each_member_listed_models_not_the_lead_catalog() {
    let (_dir, db) = test_db();
    db.set_setting(FEATURE_ROUTE_POOL_V2, "true").unwrap();
    db.set_setting(FEATURE_ROUTE_INDEX_V2, "true").unwrap();
    let providers = ProviderRepo::new(db.clone());
    providers
        .create(&openai_source_with_listed(
            "openai-a",
            "sk-openai-a",
            &["m1"],
        ))
        .unwrap();
    providers
        .create(&openai_source_with_listed(
            "openai-b",
            "sk-openai-b",
            &["m2"],
        ))
        .unwrap();
    let service = AdapterBridgeService::new(db.clone());
    let prepared = service
        .prepare(&openai_request(AdapterSourceKind::Provider, "openai-a"))
        .unwrap();
    let pools = RoutePoolService::new(db.clone());
    pools
        .add_member(
            &prepared.profile().id,
            AdapterSourceKind::Provider,
            "openai-b",
        )
        .unwrap();
    pools
        .enroll_unified_gateway(&prepared.profile().id, 43155)
        .unwrap();
    let prepared = service
        .prepare(&openai_request(AdapterSourceKind::Provider, "openai-a"))
        .unwrap();
    let index = prepared
        .runtime_material()
        .start_spec(Some(0))
        .route_index
        .expect("enrolled pool attaches production index");
    assert_eq!(index.list_models("responses"), vec!["m1", "m2"]);
    let m1 = index.resolve("responses", "m1").expect("m1");
    assert_eq!(
        m1.iter()
            .map(|candidate| candidate.member_id.as_str())
            .collect::<Vec<_>>(),
        vec!["openai-a"]
    );
    let m2 = index.resolve("responses", "m2").expect("m2");
    assert_eq!(
        m2.iter()
            .map(|candidate| candidate.member_id.as_str())
            .collect::<Vec<_>>(),
        vec!["openai-b"]
    );
}

#[test]
fn production_index_labels_members_by_their_own_provider() {
    let (_dir, db) = test_db();
    db.set_setting(FEATURE_ROUTE_POOL_V2, "true").unwrap();
    db.set_setting(FEATURE_ROUTE_INDEX_V2, "true").unwrap();
    db.set_setting(FEATURE_MIXED_PROVIDER_POOL, "true").unwrap();
    AccountRepo::new(db.clone())
        .create(&grok_subscription_account(
            "grok-subscription",
            "grok-upstream-secret",
        ))
        .unwrap();
    AccountRepo::new(db.clone())
        .create(&codex_subscription_account(
            "codex-subscription",
            "codex-upstream-access-secret",
        ))
        .unwrap();
    let service = AdapterBridgeService::new(db.clone());
    let prepared = service
        .prepare(&grok_codex_account_request("grok-subscription"))
        .unwrap();
    let pools = RoutePoolService::new(db.clone());
    pools
        .add_member(
            &prepared.profile().id,
            AdapterSourceKind::Account,
            "codex-subscription",
        )
        .unwrap();
    pools
        .add_rule(
            &prepared.profile().id,
            "m1",
            "responses",
            "grok",
            "grok",
            "grok-4.5",
            0,
            Some("shared"),
        )
        .unwrap();
    pools
        .add_rule(
            &prepared.profile().id,
            "m1",
            "responses",
            "codex",
            "codex",
            "gpt-5.4",
            10,
            Some("shared"),
        )
        .unwrap();
    pools
        .enroll_unified_gateway(&prepared.profile().id, 43155)
        .unwrap();
    let prepared = service
        .prepare(&grok_codex_account_request("grok-subscription"))
        .unwrap();
    let index = prepared
        .runtime_material()
        .start_spec(Some(0))
        .route_index
        .expect("enrolled mixed pool attaches index");
    let providers: Vec<_> = index
        .capability_snapshots()
        .into_iter()
        .map(|snapshot| (snapshot.member_id, snapshot.upstream_provider))
        .collect();
    assert!(
        providers
            .iter()
            .any(|(id, provider)| id == "grok-subscription" && provider == "grok"),
        "grok member must keep grok provider, got {providers:?}"
    );
    assert!(
        providers
            .iter()
            .any(|(id, provider)| id == "codex-subscription" && provider == "codex"),
        "codex member must not inherit the grok lead provider, got {providers:?}"
    );
}

#[test]
fn production_index_omits_sibling_when_member_snapshot_fails() {
    let (_dir, db) = test_db();
    db.set_setting(FEATURE_ROUTE_POOL_V2, "true").unwrap();
    db.set_setting(FEATURE_ROUTE_INDEX_V2, "true").unwrap();
    ProviderRepo::new(db.clone())
        .create(&openai_source_with_listed(
            "openai-a",
            "sk-openai-a",
            &["m1"],
        ))
        .unwrap();
    let service = AdapterBridgeService::new(db.clone());
    let prepared = service
        .prepare(&openai_request(AdapterSourceKind::Provider, "openai-a"))
        .unwrap();
    let pools = RoutePoolService::new(db.clone());
    pools
        .add_member(
            &prepared.profile().id,
            AdapterSourceKind::Provider,
            "openai-missing",
        )
        .unwrap();
    pools
        .enroll_unified_gateway(&prepared.profile().id, 43155)
        .unwrap();
    let prepared = service
        .prepare(&openai_request(AdapterSourceKind::Provider, "openai-a"))
        .unwrap();
    let index = prepared
        .runtime_material()
        .start_spec(Some(0))
        .route_index
        .expect("lead snapshot still builds an index");
    assert_eq!(index.list_models("responses"), vec!["m1"]);
    assert!(index.resolve("responses", "m2").is_err());
}

#[test]
fn attach_keeps_last_successful_sibling_when_prior_index_is_present() {
    let (_dir, db) = test_db();
    db.set_setting(FEATURE_ROUTE_POOL_V2, "true").unwrap();
    db.set_setting(FEATURE_ROUTE_INDEX_V2, "true").unwrap();
    let providers = ProviderRepo::new(db.clone());
    providers
        .create(&openai_source_with_listed(
            "openai-a",
            "sk-openai-a",
            &["m1"],
        ))
        .unwrap();
    providers
        .create(&openai_source_with_listed(
            "openai-b",
            "sk-openai-b",
            &["m2"],
        ))
        .unwrap();
    let service = AdapterBridgeService::new(db.clone());
    let prepared = service
        .prepare(&openai_request(AdapterSourceKind::Provider, "openai-a"))
        .unwrap();
    let pools = RoutePoolService::new(db.clone());
    pools
        .add_member(
            &prepared.profile().id,
            AdapterSourceKind::Provider,
            "openai-b",
        )
        .unwrap();
    pools
        .enroll_unified_gateway(&prepared.profile().id, 43155)
        .unwrap();
    let prepared = service
        .prepare(&openai_request(AdapterSourceKind::Provider, "openai-a"))
        .unwrap();
    let prior = prepared.runtime_material().clone();
    assert_eq!(
        prior
            .route_index()
            .expect("enrolled")
            .list_models("responses"),
        vec!["m1", "m2"]
    );
    providers.delete("openai-b").unwrap();
    let kept = service
        .attach_route_index(prior, prepared.profile())
        .unwrap();
    let index = kept
        .route_index()
        .expect("partial rebuild keeps last-successful sibling");
    assert_eq!(index.list_models("responses"), vec!["m1", "m2"]);
    assert_eq!(
        index
            .resolve("responses", "m2")
            .expect("kept B")
            .iter()
            .map(|candidate| candidate.member_id.as_str())
            .collect::<Vec<_>>(),
        vec!["openai-b"]
    );
}

#[test]
fn pool_listener_spec_lists_models_from_enabled_member() {
    let (_dir, db) = test_db();
    db.set_setting(FEATURE_ROUTE_POOL_V2, "true").unwrap();
    ProviderRepo::new(db.clone())
        .create(&kimi_source(
            "kimi-membership",
            "upstream-membership-secret",
        ))
        .unwrap();
    let pools = RoutePoolService::new(db.clone());
    let pool = pools
        .ensure_default_pool(AgentId::Codex, RouteDownstreamSurface::Responses)
        .unwrap();
    let empty = AdapterBridgeService::new(db.clone()).pool_listener_spec(&pool, (false, false));
    assert!(empty.listed_models.is_empty());

    pools
        .add_member(&pool.id, AdapterSourceKind::Provider, "kimi-membership")
        .unwrap();
    let spec = AdapterBridgeService::new(db).pool_listener_spec(&pool, (false, false));
    assert!(!spec.listed_models.is_empty());
    assert!(spec.upstream.auth.has_token());
    assert_ne!(spec.upstream.base_url, "http://127.0.0.1/");
}
