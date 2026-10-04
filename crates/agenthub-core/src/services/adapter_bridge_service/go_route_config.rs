use std::collections::HashSet;
use std::fmt;
use std::net::IpAddr;

use serde::Serialize;

use super::AdapterBridgeService;
use crate::bridge::{
    BridgeMemberSpec, BridgeStartSpec, BridgeUpstreamProtocol, MemberCapabilitySnapshot,
};
use crate::error::{AppError, Result};
use crate::models::{RouteDownstreamSurface, RoutePool};

const CONFIG_VERSION: &str = "route-config.v0-isolated";
const PRODUCT_DEFAULT_PORT: u16 = 43121;

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
    surface: &'static str,
    dialect: &'static str,
    schedule_policy: &'static str,
    fixture_model: String,
    members: Vec<GoRouteIsolatedMember>,
}

#[derive(Serialize)]
struct GoRouteIsolatedMember {
    id: String,
    upstream_base_url: String,
    upstream_key: String,
    upstream_auth: &'static str,
    priority: i64,
    position: i64,
    models: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    quota_remaining_pct: Option<f64>,
}

impl AdapterBridgeService {
    /// Resolve saved RoutePools into the isolated Go runner's in-memory config.
    ///
    /// The returned bytes contain live login information. Callers must hand them
    /// directly to the child process and must not log, persist, or return them to
    /// a command/UI boundary.
    pub fn build_go_route_isolated_config(&self, pools: &[RoutePool]) -> Result<Vec<u8>> {
        let flags = self.route_pools.pair_adapter_flags();
        let mut edges = Vec::with_capacity(pools.len());
        for pool in pools {
            let enabled_member_count = self
                .route_pools
                .list_members(&pool.id)
                .map_err(|_| incompatible_go_config_error())?
                .into_iter()
                .filter(|member| member.enabled)
                .count();
            let spec = self.pool_listener_spec(pool, flags);
            if enabled_member_count == 0 || spec.members.len() != enabled_member_count {
                return Err(incompatible_go_config_error());
            }
            edges.push(go_edge_from_spec(pool, &spec)?);
        }
        if edges.is_empty() {
            return Err(go_config_error(
                "No saved route is available for the isolated Go runtime.",
            ));
        }

        serde_json::to_vec(&GoRouteIsolatedConfig {
            version: CONFIG_VERSION,
            edges,
        })
        .map_err(|_| go_config_error("The isolated Go route configuration could not be built."))
    }
}

fn go_edge_from_spec(pool: &RoutePool, spec: &BridgeStartSpec) -> Result<GoRouteIsolatedEdge> {
    if spec.local_token.trim().is_empty() {
        return Err(incompatible_go_config_error());
    }
    let members = if let Some(index) = spec.route_index.as_ref() {
        let snapshots = index.capability_snapshots();
        spec.members
            .iter()
            .map(|member| indexed_member(pool.downstream_surface, member, &snapshots))
            .collect::<Result<Vec<_>>>()?
    } else {
        let upstream_auth = compatible_auth(pool.downstream_surface, spec.upstream.protocol)
            .ok_or_else(incompatible_go_config_error)?;
        if !is_loopback_upstream(&spec.upstream.base_url) {
            return Err(incompatible_go_config_error());
        }
        spec.members
            .iter()
            .map(|member| {
                flat_member(
                    member,
                    &spec.upstream.base_url,
                    upstream_auth,
                    spec.listed_models.clone(),
                )
            })
            .collect::<Result<Vec<_>>>()?
    };
    let fixture_model = members
        .iter()
        .find_map(|member| member.models.first())
        .ok_or_else(incompatible_go_config_error)?
        .clone();

    Ok(GoRouteIsolatedEdge {
        id: pool.id.clone(),
        ingress_key: spec.local_token.clone(),
        surface: pool.downstream_surface.as_str(),
        dialect: pool.downstream_dialect.as_str(),
        schedule_policy: pool.schedule_policy.as_str(),
        fixture_model,
        members,
    })
}

fn indexed_member(
    surface: RouteDownstreamSurface,
    member: &BridgeMemberSpec,
    snapshots: &[MemberCapabilitySnapshot],
) -> Result<GoRouteIsolatedMember> {
    let mut upstream_base_url: Option<&str> = None;
    let mut upstream_auth = None;
    let mut models = Vec::new();
    let mut seen_models = HashSet::new();

    for snapshot in snapshots
        .iter()
        .filter(|snapshot| snapshot.member_id == member.source_id)
    {
        if snapshot.public_model.trim() != snapshot.upstream_model.trim() {
            return Err(incompatible_go_config_error());
        }
        let auth = compatible_transport(surface, &snapshot.transport_key)
            .ok_or_else(incompatible_go_config_error)?;
        if !is_loopback_upstream(&snapshot.upstream_endpoint) {
            return Err(incompatible_go_config_error());
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
        let model = snapshot.public_model.trim();
        if !model.is_empty() && seen_models.insert(model.to_owned()) {
            models.push(model.to_owned());
        }
    }

    flat_member(
        member,
        upstream_base_url.ok_or_else(incompatible_go_config_error)?,
        upstream_auth.ok_or_else(incompatible_go_config_error)?,
        models,
    )
}

fn flat_member(
    member: &BridgeMemberSpec,
    upstream_base_url: &str,
    upstream_auth: &'static str,
    models: Vec<String>,
) -> Result<GoRouteIsolatedMember> {
    let source_id = member.source_id.trim();
    let id = if member.ticket_id.trim().is_empty() {
        let source_kind = member.source_kind.trim();
        if source_kind.is_empty() || source_id.is_empty() {
            return Err(incompatible_go_config_error());
        }
        format!("{source_kind}:{source_id}")
    } else {
        member.ticket_id.trim().to_owned()
    };
    let upstream_key = member.auth.token();
    if source_id.is_empty() || upstream_key.trim().is_empty() || models.is_empty() {
        return Err(incompatible_go_config_error());
    }
    Ok(GoRouteIsolatedMember {
        id,
        upstream_base_url: upstream_base_url.trim_end_matches('/').to_owned(),
        upstream_key,
        upstream_auth,
        priority: member.priority,
        position: member.position,
        models,
        quota_remaining_pct: member.quota_remaining_pct.filter(|value| value.is_finite()),
    })
}

fn compatible_auth(
    surface: RouteDownstreamSurface,
    protocol: BridgeUpstreamProtocol,
) -> Option<&'static str> {
    match (surface, protocol) {
        (RouteDownstreamSurface::Messages, BridgeUpstreamProtocol::AnthropicMessages) => {
            Some("x_api_key")
        }
        (
            RouteDownstreamSurface::Responses,
            BridgeUpstreamProtocol::CodexResponsesOauth | BridgeUpstreamProtocol::XaiResponsesOauth,
        )
        | (
            RouteDownstreamSurface::ChatCompletions,
            BridgeUpstreamProtocol::OpenAiChatCompletions,
        ) => Some("bearer"),
        _ => None,
    }
}

fn compatible_transport(
    surface: RouteDownstreamSurface,
    transport_key: &str,
) -> Option<&'static str> {
    match (surface, transport_key.trim()) {
        (RouteDownstreamSurface::Messages, "anthropic:claude") => Some("x_api_key"),
        (RouteDownstreamSurface::Responses, "codex:codex" | "grok:grok")
        | (RouteDownstreamSurface::ChatCompletions, "openai:generic") => Some("bearer"),
        _ => None,
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
