use std::collections::{BTreeSet, HashSet};
use std::fmt;
use std::net::IpAddr;

use serde::Serialize;
use sha2::{Digest, Sha256};

use super::AdapterBridgeService;
use crate::bridge::{
    BridgeMemberSpec, BridgeStartSpec, BridgeUpstreamProtocol, MemberCapabilitySnapshot,
};
use crate::error::{AppError, Result};
use crate::models::{AdapterSourceKind, AdapterSourceProduct, RouteDownstreamSurface, RoutePool};

const CONFIG_VERSION: &str = "route-config.v0-isolated";
const PRODUCT_DEFAULT_PORT: u16 = 43121;
const TRANSPORT_ANTHROPIC_MESSAGES: &str = "anthropic_messages";
const TRANSPORT_CODEX_RESPONSES: &str = "codex_responses";
const TRANSPORT_GROK_RESPONSES: &str = "grok_responses";
const TRANSPORT_OPENAI_CHAT_COMPLETIONS: &str = "openai_chat_completions";
const UPSTREAM_TARGET_ANTHROPIC_API: &str = "anthropic_api";
const UPSTREAM_TARGET_OPENAI_API: &str = "openai_api";
const UPSTREAM_TARGET_KIMI_CODE_MEMBERSHIP: &str = "kimi_code_membership";
const UPSTREAM_TARGET_CODEX_CHATGPT_SUBSCRIPTION: &str = "codex_chatgpt_subscription";
const UPSTREAM_TARGET_GROK_XAI_SUBSCRIPTION: &str = "grok_xai_subscription";
const UPSTREAM_TARGET_LOOPBACK: &str = "loopback";
const CREDENTIAL_CLASS_API_KEY: &str = "api_key";
const CREDENTIAL_CLASS_OFFICIAL_LOGIN: &str = "official_login";
const CREDENTIAL_CLASS_LOCAL: &str = "local";
const REFRESH_NONE: &str = "none";
const REFRESH_CODEX_OAUTH: &str = "codex_oauth";
const REFRESH_GROK_OAUTH: &str = "grok_oauth";

/// Stable outcome of the read-only Product Go route eligibility check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GoProductPreflightReason {
    NoPool,
    MissingSavedPort,
    ZeroSavedPort,
    MultipleSavedPorts,
    UncoveredLegacyProfile,
    ProfilePortMismatch,
    ConfigIncompatible,
    Eligible,
}

impl GoProductPreflightReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NoPool => "no_pool",
            Self::MissingSavedPort => "missing_saved_port",
            Self::ZeroSavedPort => "zero_saved_port",
            Self::MultipleSavedPorts => "multiple_saved_ports",
            Self::UncoveredLegacyProfile => "uncovered_legacy_profile",
            Self::ProfilePortMismatch => "profile_port_mismatch",
            Self::ConfigIncompatible => "config_incompatible",
            Self::Eligible => "eligible",
        }
    }
}

/// Credential-free Product eligibility evidence suitable for command/UI DTOs.
///
/// The complete generated configuration is deliberately absent: it contains
/// live login information and only stays inside [`PreparedGoProductConfig`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoProductPreflightSummary {
    pub reason: GoProductPreflightReason,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    pub pool_count: usize,
    pub legacy_profile_count: usize,
    pub uncovered_profile_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_len: Option<usize>,
}

impl GoProductPreflightSummary {
    pub fn eligible(&self) -> bool {
        self.reason == GoProductPreflightReason::Eligible
    }
}

/// In-process handoff from Core to the desktop supervisor.
///
/// This type is intentionally not serializable and redacts the configuration
/// from `Debug`. Only an eligible result carries configuration bytes.
pub struct PreparedGoProductConfig {
    summary: GoProductPreflightSummary,
    config: Option<Vec<u8>>,
}

impl fmt::Debug for PreparedGoProductConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedGoProductConfig")
            .field("summary", &self.summary)
            .field("config", &self.config.as_ref().map(Vec::len))
            .finish()
    }
}

impl PreparedGoProductConfig {
    pub fn summary(&self) -> &GoProductPreflightSummary {
        &self.summary
    }

    pub fn into_config(self) -> Option<Vec<u8>> {
        self.config
    }
}

#[derive(Serialize)]
struct GoRouteIsolatedConfig {
    version: &'static str,
    edges: Vec<GoRouteIsolatedEdge>,
}

impl fmt::Debug for GoRouteIsolatedConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GoRouteIsolatedConfig")
            .field("version", &self.version)
            .field("edge_count", &self.edges.len())
            .finish()
    }
}

#[derive(Serialize)]
struct GoRouteIsolatedEdge {
    id: String,
    ingress_key: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    ingress_keys: Vec<String>,
    surface: &'static str,
    dialect: &'static str,
    codex_ingress_grok_upstream: bool,
    grok_ingress_codex_upstream: bool,
    schedule_policy: &'static str,
    fixture_model: String,
    members: Vec<GoRouteIsolatedMember>,
}

#[derive(Serialize)]
struct GoRouteIsolatedMember {
    id: String,
    source_kind: String,
    source_id: String,
    refresh_kind: &'static str,
    upstream_base_url: String,
    upstream_key: String,
    upstream_auth: &'static str,
    upstream_transport: &'static str,
    upstream_target: &'static str,
    credential_class: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    official_account_id: Option<String>,
    priority: i64,
    position: i64,
    models: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    quota_remaining_pct: Option<f64>,
}

#[derive(Clone, Copy)]
struct TrustedUpstreamSource {
    target: &'static str,
    credential_class: &'static str,
    index_provider: Option<&'static str>,
}

impl AdapterBridgeService {
    /// Evaluate whether the persisted route state can be handed to the dormant
    /// Product Go supervisor without changing that state or opening a socket.
    ///
    /// A legacy profile is covered when an included listener pool has the same
    /// target/surface and an enabled member with the profile's source identity.
    /// Pool ids are not a relationship key: a source can legitimately be
    /// represented in more than one matching pool.
    pub fn prepare_go_product_config(&self) -> Result<PreparedGoProductConfig> {
        let port_integrity = self.route_pools.gateway_listener_port_integrity()?;
        if port_integrity.count == 0 {
            return Ok(product_preflight(
                GoProductPreflightReason::NoPool,
                None,
                0,
                0,
                0,
            ));
        }
        if port_integrity.has_missing {
            return Ok(product_preflight(
                GoProductPreflightReason::MissingSavedPort,
                None,
                port_integrity.count,
                0,
                0,
            ));
        }
        if port_integrity.has_zero {
            return Ok(product_preflight(
                GoProductPreflightReason::ZeroSavedPort,
                None,
                port_integrity.count,
                0,
                0,
            ));
        }
        let pools = self.route_pools.list_gateway_listener_pools()?;
        let pool_count = pools.len();
        if pool_count != port_integrity.count {
            return Err(AppError::message(
                "adapter.go_product_preflight_stale",
                "saved route state changed during Product preflight",
            ));
        }
        let ports = pools
            .iter()
            .filter_map(|pool| pool.gateway_port)
            .collect::<HashSet<_>>();
        if ports.len() != 1 {
            return Ok(product_preflight(
                GoProductPreflightReason::MultipleSavedPorts,
                None,
                pool_count,
                0,
                0,
            ));
        }
        let port = ports.into_iter().next();

        let profiles = self.list_auto_start_profiles()?;
        let mut uncovered_profile_count = 0;
        for profile in &profiles {
            let expected_surface = RouteDownstreamSurface::for_agent(profile.target_agent_id);
            let mut covered = false;
            for pool in pools.iter().filter(|pool| {
                pool.target_agent_id == profile.target_agent_id
                    && Some(pool.downstream_surface) == expected_surface
            }) {
                if self
                    .route_pools
                    .list_members(&pool.id)?
                    .into_iter()
                    .any(|member| {
                        member.enabled
                            && member.source_kind == profile.source_kind
                            && member.source_id == profile.source_id
                    })
                {
                    covered = true;
                    break;
                }
            }
            if !covered {
                uncovered_profile_count += 1;
            }
        }
        if uncovered_profile_count != 0 {
            return Ok(product_preflight(
                GoProductPreflightReason::UncoveredLegacyProfile,
                port,
                pool_count,
                profiles.len(),
                uncovered_profile_count,
            ));
        }
        if profiles.iter().any(|profile| profile.local_port != port) {
            return Ok(product_preflight(
                GoProductPreflightReason::ProfilePortMismatch,
                port,
                pool_count,
                profiles.len(),
                0,
            ));
        }

        let config = match self.build_go_route_config(&pools, true) {
            Ok(config) => config,
            Err(_) => {
                return Ok(product_preflight(
                    GoProductPreflightReason::ConfigIncompatible,
                    port,
                    pool_count,
                    profiles.len(),
                    0,
                ));
            }
        };
        let config_sha256 = format!("{:x}", Sha256::digest(&config));
        Ok(PreparedGoProductConfig {
            summary: GoProductPreflightSummary {
                reason: GoProductPreflightReason::Eligible,
                port,
                pool_count,
                legacy_profile_count: profiles.len(),
                uncovered_profile_count: 0,
                config_sha256: Some(config_sha256),
                config_len: Some(config.len()),
            },
            config: Some(config),
        })
    }

    /// Resolve saved RoutePools into the isolated Go runner's in-memory config.
    ///
    /// The returned bytes contain live login information. Callers must hand them
    /// directly to the child process and must not log, persist, or return them to
    /// a command/UI boundary.
    pub fn build_go_route_isolated_config(&self, pools: &[RoutePool]) -> Result<Vec<u8>> {
        self.build_go_route_config(pools, false)
    }

    /// Resolve an auth-refresh request against the complete configuration that
    /// would be handed to Go now. Every caller-supplied identity field must
    /// match the same current member; the returned account id is read from that
    /// rebuilt configuration and never from the untrusted request alone.
    pub fn resolve_go_route_oauth_refresh(
        &self,
        edge_id: &str,
        member_id: &str,
        source_id: &str,
        refresh_kind: &str,
    ) -> Result<String> {
        let pools = self
            .route_pools
            .list_gateway_listener_pools()
            .map_err(|_| go_oauth_refresh_rejected())?;
        let config = self
            .build_go_route_config_model(&pools, false)
            .map_err(|_| go_oauth_refresh_rejected())?;
        let edge_id = edge_id.trim();
        let member_id = member_id.trim();
        let source_id = source_id.trim();
        let refresh_kind = refresh_kind.trim();
        let mut matching = config
            .edges
            .iter()
            .filter(|edge| edge.id == edge_id)
            .flat_map(|edge| edge.members.iter())
            .filter(|member| {
                member.id == member_id
                    && member.source_id == source_id
                    && member.refresh_kind == refresh_kind
            });
        let member = matching.next().ok_or_else(go_oauth_refresh_rejected)?;
        if matching.next().is_some() {
            return Err(go_oauth_refresh_rejected());
        }
        let eligible_transport = matches!(
            (member.refresh_kind, member.upstream_transport),
            (REFRESH_CODEX_OAUTH, TRANSPORT_CODEX_RESPONSES)
                | (REFRESH_GROK_OAUTH, TRANSPORT_GROK_RESPONSES)
        );
        if member.source_kind != "account" || !eligible_transport {
            return Err(go_oauth_refresh_rejected());
        }
        Ok(member.source_id.clone())
    }

    fn build_go_route_config(&self, pools: &[RoutePool], read_only: bool) -> Result<Vec<u8>> {
        let config = self.build_go_route_config_model(pools, read_only)?;
        serde_json::to_vec(&config)
            .map_err(|_| go_config_error("The isolated Go route configuration could not be built."))
    }

    fn build_go_route_config_model(
        &self,
        pools: &[RoutePool],
        read_only: bool,
    ) -> Result<GoRouteIsolatedConfig> {
        let flags = self.route_pools.pair_adapter_flags();
        let accepted_bearers = self
            .route_pools
            .list_accepted_local_bearers()
            .map_err(|_| incompatible_go_config_error())?;
        let mut edges = Vec::with_capacity(pools.len());
        for pool in pools {
            let enabled_member_count = self
                .route_pools
                .list_members(&pool.id)
                .map_err(|_| incompatible_go_config_error())?
                .into_iter()
                .filter(|member| member.enabled)
                .count();
            let spec = if read_only {
                self.pool_listener_spec_read_only(pool, flags)
            } else {
                self.pool_listener_spec(pool, flags)
            };
            if enabled_member_count == 0 || spec.members.len() != enabled_member_count {
                return Err(incompatible_go_config_error());
            }
            edges.push(go_edge_from_spec(self, pool, &spec, &accepted_bearers)?);
        }
        if edges.is_empty() {
            return Err(go_config_error(
                "No saved route is available for the isolated Go runtime.",
            ));
        }

        Ok(GoRouteIsolatedConfig {
            version: CONFIG_VERSION,
            edges,
        })
    }
}

fn product_preflight(
    reason: GoProductPreflightReason,
    port: Option<u16>,
    pool_count: usize,
    legacy_profile_count: usize,
    uncovered_profile_count: usize,
) -> PreparedGoProductConfig {
    PreparedGoProductConfig {
        summary: GoProductPreflightSummary {
            reason,
            port,
            pool_count,
            legacy_profile_count,
            uncovered_profile_count,
            config_sha256: None,
            config_len: None,
        },
        config: None,
    }
}

fn go_edge_from_spec(
    service: &AdapterBridgeService,
    pool: &RoutePool,
    spec: &BridgeStartSpec,
    accepted_bearers: &[(String, String)],
) -> Result<GoRouteIsolatedEdge> {
    if spec.local_token.trim().is_empty() {
        return Err(incompatible_go_config_error());
    }
    // Keep the pool's primary key in the required compatibility field. Sort
    // and deduplicate every persisted extra or historical same-pool projection
    // in the aliases field. Stable ordering is important because the desktop
    // hashes the complete serialized config.
    let primary = spec.local_token.trim().to_owned();
    let aliases = accepted_bearers
        .iter()
        .filter(|(_, pool_id)| pool_id == &pool.id)
        .filter_map(|(bearer, _)| {
            let bearer = bearer.trim();
            (!bearer.is_empty() && bearer != primary).then(|| bearer.to_owned())
        })
        .collect::<BTreeSet<_>>();
    let ingress_keys = aliases.into_iter().collect();
    let members = if let Some(index) = spec.route_index.as_ref() {
        let snapshots = index.capability_snapshots();
        spec.members
            .iter()
            .map(|member| indexed_member(service, pool.downstream_surface, member, &snapshots))
            .collect::<Result<Vec<_>>>()?
    } else {
        let (upstream_auth, upstream_transport) =
            compatible_protocol(pool.downstream_surface, spec.upstream.protocol)
                .ok_or_else(incompatible_go_config_error)?;
        spec.members
            .iter()
            .map(|member| {
                let trusted_source = upstream_source(service, member, &spec.upstream.base_url)?;
                flat_member(
                    service,
                    pool.downstream_surface,
                    member,
                    &spec.upstream.base_url,
                    upstream_auth,
                    upstream_transport,
                    spec.listed_models.clone(),
                    trusted_source,
                )
            })
            .collect::<Result<Vec<_>>>()?
    };
    let fixture_model = members
        .iter()
        .find_map(|member| member.models.first())
        .ok_or_else(incompatible_go_config_error)?
        .clone();
    let downstream_dialect = pool.downstream_dialect.as_str();
    if members.iter().any(
        |member| match (downstream_dialect, member.upstream_target) {
            ("codex", UPSTREAM_TARGET_GROK_XAI_SUBSCRIPTION) => !spec.codex_ingress_grok_upstream,
            ("grok", UPSTREAM_TARGET_CODEX_CHATGPT_SUBSCRIPTION) => {
                !spec.grok_ingress_codex_upstream
            }
            ("codex", UPSTREAM_TARGET_CODEX_CHATGPT_SUBSCRIPTION)
            | ("grok", UPSTREAM_TARGET_GROK_XAI_SUBSCRIPTION) => false,
            (
                _,
                UPSTREAM_TARGET_CODEX_CHATGPT_SUBSCRIPTION | UPSTREAM_TARGET_GROK_XAI_SUBSCRIPTION,
            ) => true,
            _ => false,
        },
    ) {
        return Err(incompatible_go_config_error());
    }

    Ok(GoRouteIsolatedEdge {
        id: pool.id.clone(),
        ingress_key: primary,
        ingress_keys,
        surface: pool.downstream_surface.as_str(),
        dialect: downstream_dialect,
        codex_ingress_grok_upstream: spec.codex_ingress_grok_upstream,
        grok_ingress_codex_upstream: spec.grok_ingress_codex_upstream,
        schedule_policy: pool.schedule_policy.as_str(),
        fixture_model,
        members,
    })
}

fn indexed_member(
    service: &AdapterBridgeService,
    surface: RouteDownstreamSurface,
    member: &BridgeMemberSpec,
    snapshots: &[MemberCapabilitySnapshot],
) -> Result<GoRouteIsolatedMember> {
    let mut trusted_source: Option<TrustedUpstreamSource> = None;
    let mut upstream_base_url: Option<&str> = None;
    let mut upstream_auth = None;
    let mut upstream_transport = None;
    let mut models = Vec::new();
    let mut seen_models = HashSet::new();

    for snapshot in snapshots
        .iter()
        .filter(|snapshot| snapshot.member_id == member.source_id)
    {
        let snapshot_source = upstream_source(service, member, &snapshot.upstream_endpoint)?;
        if snapshot_source
            .index_provider
            .is_some_and(|provider| snapshot.upstream_provider.trim() != provider)
        {
            return Err(incompatible_go_config_error());
        }
        if snapshot.public_model.trim() != snapshot.upstream_model.trim() {
            return Err(incompatible_go_config_error());
        }
        let (auth, transport) = compatible_transport(surface, &snapshot.transport_key)
            .ok_or_else(incompatible_go_config_error)?;
        if !is_allowed_upstream(
            &snapshot.upstream_endpoint,
            surface,
            transport,
            snapshot_source.target,
        ) {
            return Err(incompatible_go_config_error());
        }
        match trusted_source {
            Some(existing)
                if existing.target != snapshot_source.target
                    || existing.credential_class != snapshot_source.credential_class =>
            {
                return Err(incompatible_go_config_error())
            }
            None => trusted_source = Some(snapshot_source),
            _ => {}
        }
        match upstream_base_url {
            Some(existing) if !same_upstream(existing, &snapshot.upstream_endpoint) => {
                return Err(incompatible_go_config_error())
            }
            None => upstream_base_url = Some(&snapshot.upstream_endpoint),
            _ => {}
        }
        match upstream_auth {
            Some(existing) if existing != auth => return Err(incompatible_go_config_error()),
            None => upstream_auth = Some(auth),
            _ => {}
        }
        match upstream_transport {
            Some(existing) if existing != transport => return Err(incompatible_go_config_error()),
            None => upstream_transport = Some(transport),
            _ => {}
        }
        let model = snapshot.public_model.trim();
        if !model.is_empty() && seen_models.insert(model.to_owned()) {
            models.push(model.to_owned());
        }
    }

    flat_member(
        service,
        surface,
        member,
        upstream_base_url.ok_or_else(incompatible_go_config_error)?,
        upstream_auth.ok_or_else(incompatible_go_config_error)?,
        upstream_transport.ok_or_else(incompatible_go_config_error)?,
        models,
        trusted_source.ok_or_else(incompatible_go_config_error)?,
    )
}

fn flat_member(
    service: &AdapterBridgeService,
    surface: RouteDownstreamSurface,
    member: &BridgeMemberSpec,
    upstream_base_url: &str,
    upstream_auth: &'static str,
    upstream_transport: &'static str,
    models: Vec<String>,
    trusted_source: TrustedUpstreamSource,
) -> Result<GoRouteIsolatedMember> {
    let official_account_id = official_account_id(service, member, trusted_source)?;
    if !is_allowed_upstream(
        upstream_base_url,
        surface,
        upstream_transport,
        trusted_source.target,
    ) {
        return Err(incompatible_go_config_error());
    }
    let source_kind = member.source_kind.trim();
    let source_id = member.source_id.trim();
    let id = if member.ticket_id.trim().is_empty() {
        if source_kind.is_empty() || source_id.is_empty() {
            return Err(incompatible_go_config_error());
        }
        format!("{source_kind}:{source_id}")
    } else {
        member.ticket_id.trim().to_owned()
    };
    let upstream_key = member.auth.token();
    if !matches!(source_kind, "account" | "provider")
        || source_id.is_empty()
        || upstream_key.trim().is_empty()
        || models.is_empty()
    {
        return Err(incompatible_go_config_error());
    }
    let refresh_kind = refresh_kind(service, member, upstream_transport);
    match trusted_source.target {
        UPSTREAM_TARGET_CODEX_CHATGPT_SUBSCRIPTION
            if refresh_kind != REFRESH_CODEX_OAUTH || official_account_id.is_none() =>
        {
            return Err(incompatible_go_config_error());
        }
        UPSTREAM_TARGET_GROK_XAI_SUBSCRIPTION if refresh_kind != REFRESH_GROK_OAUTH => {
            return Err(incompatible_go_config_error());
        }
        _ => {}
    }
    Ok(GoRouteIsolatedMember {
        id,
        source_kind: source_kind.to_owned(),
        source_id: source_id.to_owned(),
        refresh_kind,
        upstream_base_url: upstream_base_url.trim_end_matches('/').to_owned(),
        upstream_key,
        upstream_auth,
        upstream_transport,
        upstream_target: trusted_source.target,
        credential_class: trusted_source.credential_class,
        official_account_id,
        priority: member.priority,
        position: member.position,
        models,
        quota_remaining_pct: member.quota_remaining_pct.filter(|value| value.is_finite()),
    })
}

fn official_account_id(
    service: &AdapterBridgeService,
    member: &BridgeMemberSpec,
    trusted_source: TrustedUpstreamSource,
) -> Result<Option<String>> {
    if trusted_source.target != UPSTREAM_TARGET_CODEX_CHATGPT_SUBSCRIPTION
        || trusted_source.credential_class != CREDENTIAL_CLASS_OFFICIAL_LOGIN
    {
        return Ok(None);
    }
    let source_kind = AdapterSourceKind::parse(member.source_kind.trim())
        .ok_or_else(incompatible_go_config_error)?;
    service
        .secrets
        .resolve_codex_subscription_account_id(source_kind, member.source_id.trim())
        .map(Some)
        .map_err(|_| incompatible_go_config_error())
}

fn upstream_source(
    service: &AdapterBridgeService,
    member: &BridgeMemberSpec,
    upstream_base_url: &str,
) -> Result<TrustedUpstreamSource> {
    if is_loopback_upstream(upstream_base_url) {
        return Ok(TrustedUpstreamSource {
            target: UPSTREAM_TARGET_LOOPBACK,
            credential_class: CREDENTIAL_CLASS_LOCAL,
            index_provider: None,
        });
    }
    let source_kind =
        AdapterSourceKind::parse(&member.source_kind).ok_or_else(incompatible_go_config_error)?;
    let product = service
        .routes
        .classify_source_product(source_kind, member.source_id.trim())
        .map_err(|_| incompatible_go_config_error())?;
    match product {
        AdapterSourceProduct::AnthropicApi => Ok(TrustedUpstreamSource {
            target: UPSTREAM_TARGET_ANTHROPIC_API,
            credential_class: CREDENTIAL_CLASS_API_KEY,
            index_provider: Some("anthropic"),
        }),
        AdapterSourceProduct::OpenaiApi => Ok(TrustedUpstreamSource {
            target: UPSTREAM_TARGET_OPENAI_API,
            credential_class: CREDENTIAL_CLASS_API_KEY,
            index_provider: Some("openai"),
        }),
        AdapterSourceProduct::KimiCodeMembership => Ok(TrustedUpstreamSource {
            target: UPSTREAM_TARGET_KIMI_CODE_MEMBERSHIP,
            credential_class: CREDENTIAL_CLASS_API_KEY,
            index_provider: Some("kimi"),
        }),
        AdapterSourceProduct::CodexChatGptSubscription => Ok(TrustedUpstreamSource {
            target: UPSTREAM_TARGET_CODEX_CHATGPT_SUBSCRIPTION,
            credential_class: CREDENTIAL_CLASS_OFFICIAL_LOGIN,
            index_provider: Some("codex"),
        }),
        AdapterSourceProduct::XaiGrokSubscription => Ok(TrustedUpstreamSource {
            target: UPSTREAM_TARGET_GROK_XAI_SUBSCRIPTION,
            credential_class: CREDENTIAL_CLASS_OFFICIAL_LOGIN,
            index_provider: Some("grok"),
        }),
        AdapterSourceProduct::XaiApi
        | AdapterSourceProduct::GlmCodingPlan
        | AdapterSourceProduct::DeepseekApi
        | AdapterSourceProduct::ClaudeSubscription
        | AdapterSourceProduct::Kiro
        | AdapterSourceProduct::Other => Err(incompatible_go_config_error()),
    }
}

fn refresh_kind(
    service: &AdapterBridgeService,
    member: &BridgeMemberSpec,
    upstream_transport: &str,
) -> &'static str {
    if member.source_kind.trim() != "account" {
        return REFRESH_NONE;
    }
    let source_id = member.source_id.trim();
    match upstream_transport {
        TRANSPORT_CODEX_RESPONSES
            if service
                .secrets
                .resolve_codex_subscription_auth(
                    crate::models::AdapterSourceKind::Account,
                    source_id,
                )
                .is_ok() =>
        {
            REFRESH_CODEX_OAUTH
        }
        TRANSPORT_GROK_RESPONSES
            if service
                .secrets
                .resolve_grok_subscription_auth(
                    crate::models::AdapterSourceKind::Account,
                    source_id,
                )
                .is_ok() =>
        {
            REFRESH_GROK_OAUTH
        }
        _ => REFRESH_NONE,
    }
}

fn compatible_protocol(
    surface: RouteDownstreamSurface,
    protocol: BridgeUpstreamProtocol,
) -> Option<(&'static str, &'static str)> {
    match (surface, protocol) {
        (RouteDownstreamSurface::Messages, BridgeUpstreamProtocol::AnthropicMessages) => {
            Some(("x_api_key", TRANSPORT_ANTHROPIC_MESSAGES))
        }
        (RouteDownstreamSurface::Responses, BridgeUpstreamProtocol::CodexResponsesOauth) => {
            Some(("bearer", TRANSPORT_CODEX_RESPONSES))
        }
        (RouteDownstreamSurface::Responses, BridgeUpstreamProtocol::XaiResponsesOauth) => {
            Some(("bearer", TRANSPORT_GROK_RESPONSES))
        }
        (RouteDownstreamSurface::Responses, BridgeUpstreamProtocol::OpenAiChatCompletions) => {
            Some(("bearer", TRANSPORT_OPENAI_CHAT_COMPLETIONS))
        }
        (
            RouteDownstreamSurface::ChatCompletions,
            BridgeUpstreamProtocol::OpenAiChatCompletions,
        ) => Some(("bearer", TRANSPORT_OPENAI_CHAT_COMPLETIONS)),
        _ => None,
    }
}

fn compatible_transport(
    surface: RouteDownstreamSurface,
    transport_key: &str,
) -> Option<(&'static str, &'static str)> {
    match (surface, transport_key.trim()) {
        (RouteDownstreamSurface::Messages, "anthropic:claude") => {
            Some(("x_api_key", TRANSPORT_ANTHROPIC_MESSAGES))
        }
        (RouteDownstreamSurface::Responses, "codex:codex") => {
            Some(("bearer", TRANSPORT_CODEX_RESPONSES))
        }
        (RouteDownstreamSurface::Responses, "grok:grok") => {
            Some(("bearer", TRANSPORT_GROK_RESPONSES))
        }
        (RouteDownstreamSurface::Responses, "openai:generic") => {
            Some(("bearer", TRANSPORT_OPENAI_CHAT_COMPLETIONS))
        }
        (RouteDownstreamSurface::ChatCompletions, "openai:generic") => {
            Some(("bearer", TRANSPORT_OPENAI_CHAT_COMPLETIONS))
        }
        _ => None,
    }
}

fn is_allowed_upstream(
    raw: &str,
    surface: RouteDownstreamSurface,
    transport: &str,
    target: &str,
) -> bool {
    if target == UPSTREAM_TARGET_LOOPBACK {
        return is_loopback_upstream(raw) && transport_matches_surface(transport, surface);
    }
    if !trusted_target_matches_route(target, surface, transport) {
        return false;
    }
    match target {
        UPSTREAM_TARGET_ANTHROPIC_API => is_exact_https_base(raw, "api.anthropic.com", "/v1"),
        UPSTREAM_TARGET_OPENAI_API => is_exact_https_base(raw, "api.openai.com", "/v1"),
        UPSTREAM_TARGET_KIMI_CODE_MEMBERSHIP => {
            is_exact_https_base(raw, "api.kimi.com", "/coding/v1")
        }
        UPSTREAM_TARGET_CODEX_CHATGPT_SUBSCRIPTION => {
            is_exact_https_base(raw, "chatgpt.com", "/backend-api/codex")
                || raw.trim().strip_suffix('/').is_some_and(|without_slash| {
                    is_exact_https_base(without_slash, "chatgpt.com", "/backend-api/codex")
                })
        }
        UPSTREAM_TARGET_GROK_XAI_SUBSCRIPTION => {
            is_exact_https_base(raw, "cli-chat-proxy.grok.com", "/v1")
        }
        _ => false,
    }
}

fn transport_matches_surface(transport: &str, surface: RouteDownstreamSurface) -> bool {
    match surface {
        RouteDownstreamSurface::Messages => transport == TRANSPORT_ANTHROPIC_MESSAGES,
        RouteDownstreamSurface::Responses => matches!(
            transport,
            TRANSPORT_CODEX_RESPONSES
                | TRANSPORT_GROK_RESPONSES
                | TRANSPORT_OPENAI_CHAT_COMPLETIONS
        ),
        RouteDownstreamSurface::ChatCompletions => transport == TRANSPORT_OPENAI_CHAT_COMPLETIONS,
    }
}

fn trusted_target_matches_route(
    target: &str,
    surface: RouteDownstreamSurface,
    transport: &str,
) -> bool {
    match target {
        UPSTREAM_TARGET_ANTHROPIC_API => {
            surface == RouteDownstreamSurface::Messages && transport == TRANSPORT_ANTHROPIC_MESSAGES
        }
        UPSTREAM_TARGET_OPENAI_API | UPSTREAM_TARGET_KIMI_CODE_MEMBERSHIP => {
            matches!(
                surface,
                RouteDownstreamSurface::Responses | RouteDownstreamSurface::ChatCompletions
            ) && transport == TRANSPORT_OPENAI_CHAT_COMPLETIONS
        }
        UPSTREAM_TARGET_CODEX_CHATGPT_SUBSCRIPTION => {
            surface == RouteDownstreamSurface::Responses && transport == TRANSPORT_CODEX_RESPONSES
        }
        UPSTREAM_TARGET_GROK_XAI_SUBSCRIPTION => {
            surface == RouteDownstreamSurface::Responses && transport == TRANSPORT_GROK_RESPONSES
        }
        _ => false,
    }
}

fn is_loopback_upstream(raw: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(raw.trim()) else {
        return false;
    };
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port() == Some(PRODUCT_DEFAULT_PORT)
    {
        return false;
    }
    let Some(host) = url.host_str() else {
        return false;
    };
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

fn is_exact_https_base(raw: &str, expected_host: &str, expected_path: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(raw.trim()) else {
        return false;
    };
    url.scheme() == "https"
        && has_exact_https_authority(raw, expected_host)
        && url.username().is_empty()
        && url.password().is_none()
        && url
            .host_str()
            .is_some_and(|host| host.eq_ignore_ascii_case(expected_host))
        && url.port_or_known_default() == Some(443)
        && url.query().is_none()
        && url.fragment().is_none()
        && url.path() == expected_path
}

fn has_exact_https_authority(raw: &str, expected_host: &str) -> bool {
    raw.trim()
        .split_once("://")
        .and_then(|(_, remainder)| remainder.split(['/', '?', '#']).next())
        .is_some_and(|authority| {
            authority.eq_ignore_ascii_case(expected_host)
                || authority.eq_ignore_ascii_case(&format!("{expected_host}:443"))
        })
}

fn same_upstream(left: &str, right: &str) -> bool {
    left.trim_end_matches('/')
        .eq_ignore_ascii_case(right.trim_end_matches('/'))
}

fn go_config_error(message: &'static str) -> AppError {
    AppError::message("adapter.go_route_isolated_config", message)
}

fn incompatible_go_config_error() -> AppError {
    go_config_error("A saved route is not compatible with the isolated Go runtime.")
}

fn go_oauth_refresh_rejected() -> AppError {
    AppError::message(
        "adapter.go_route_oauth_refresh_rejected",
        "The Go route refresh request is stale or not eligible.",
    )
}
