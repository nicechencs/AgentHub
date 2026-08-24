/** Pure view-model for the route-detail endpoint mapping graph. No React, no IO. */
import type { AdapterProfile } from '@/lib/backend/contracts/adapter';
import type { ConnectionEntry } from '@/lib/connection-entry';
import type { MessageKey, TranslateFn } from '@/lib/i18n';
import type { RouteEndpointId } from '@/lib/route-endpoints';
import { ROUTE_ENDPOINT_HOST, routeEndpointHttpParts } from '@/lib/route-endpoints';
import {
  appliedTargetsFromProfiles,
  buildRouteDetailSourceView,
  detectUpstreamChannelFromUrl,
  hopForTestable,
  type RouteDetailSourceView,
  type RouteHopKind,
  type UpstreamChannel,
} from './adapter-route-detail-model';
import {
  CREATE_ROUTE_TARGETS,
  listLocalRouteSurfacesFromConfig,
  readCreateRouteCapabilities,
  surfaceForCreateRouteTarget,
  type CreateRouteTarget,
  type LocalRouteSurface,
} from './create-route-flow';

export type RouteGraphLinkStyle = 'solid' | 'dashed';

export type RouteGraphRow = {
  agent: CreateRouteTarget;
  /** Loopback path the client calls. */
  localPath: string;
  localEndpointId: RouteEndpointId;
  /** Full loopback URL, or null while the port is pending. */
  localUrl: string | null;
  /** Upstream base URL for this agent (per-target override, else the source base). '' when unknown. */
  upstreamBaseUrl: string;
  /** Upstream path AgentHub calls on that base. '' when the channel is unknown. */
  upstreamPath: string;
  /** upstreamBaseUrl + upstreamPath, '' when either is unknown. */
  upstreamUrl: string;
  upstreamChannel: UpstreamChannel;
  hop: RouteHopKind;
  link: RouteGraphLinkStyle;
  /** Declared/enabled on the source connection. */
  enabled: boolean;
  /** A sibling local_bridge profile already wrote this agent's config. */
  applied: boolean;
};

export type RouteGraphView = {
  source: RouteDetailSourceView;
  local: {
    host: string;
    port: number | null;
    /** `http://127.0.0.1:26275`, or '' while the port is pending. */
    origin: string;
  };
  rows: RouteGraphRow[];
};

const GRAPH_COPY = {
  linkPassthrough: '直通',
  linkConvert: '转换',
  linkForward: '转发',
} as const;

const UPSTREAM_PATHS: Record<UpstreamChannel, string> = {
  anthropic_messages: '/v1/messages',
  codex_responses: '/v1/responses',
  grok_responses: '/v1/responses',
  openai_chat: '/v1/chat/completions',
  unknown: '',
};

function graphText(t: TranslateFn | undefined, key: string, fallback: string): string {
  if (!t) return fallback;
  return t(key as MessageKey);
}

export function upstreamPathForChannel(channel: UpstreamChannel): string {
  return UPSTREAM_PATHS[channel];
}

export function routeGraphLinkStyle(hop: RouteHopKind): RouteGraphLinkStyle {
  return hop === 'passthrough' ? 'solid' : 'dashed';
}

/** Join upstream base + path, dropping one duplicated leading segment (`.../v1` + `/v1/...`). */
export function joinUpstreamUrl(baseUrl: string, path: string): string {
  const trimmedBase = baseUrl.trim();
  const trimmedPath = path.trim();
  if (!trimmedBase || !trimmedPath) return '';
  const base = trimmedBase.replace(/\/+$/, '');
  const suffix = trimmedPath.startsWith('/') ? trimmedPath : `/${trimmedPath}`;
  const firstSegment = suffix.split('/')[1] ?? '';
  const deduped = firstSegment && base.endsWith(`/${firstSegment}`)
    ? base.slice(0, base.length - firstSegment.length - 1)
    : base;
  return `${deduped}${suffix}`;
}

function graphTargetsToEmit(
  hasDeclaredEndpoints: boolean,
  surfaces: readonly LocalRouteSurface[],
): readonly CreateRouteTarget[] {
  if (hasDeclaredEndpoints) return CREATE_ROUTE_TARGETS;
  const seen = new Set<CreateRouteTarget>();
  const targets: CreateRouteTarget[] = [];
  for (const surface of surfaces) {
    if (seen.has(surface.target)) continue;
    seen.add(surface.target);
    targets.push(surface.target);
  }
  return targets;
}

function upstreamBaseFor(input: {
  missing: boolean;
  endpoints: readonly { target: CreateRouteTarget; url: string }[];
  target: CreateRouteTarget;
  sourceBaseUrl: string;
}): string {
  if (input.missing) return '';
  const perTarget = input.endpoints.find((row) => row.target === input.target)?.url.trim() ?? '';
  return perTarget || input.sourceBaseUrl;
}

export function buildRouteGraph(input: {
  profile: Pick<AdapterProfile, 'sourceKind' | 'sourceId' | 'name' | 'mode' | 'targetAgentId' | 'ruleId' | 'route'>;
  entries: readonly ConnectionEntry[];
  siblingProfiles: readonly Pick<AdapterProfile, 'sourceKind' | 'sourceId' | 'targetAgentId' | 'generatedProviderId' | 'route'>[];
  host?: string;
  port?: number | null;
}): RouteGraphView {
  const source = buildRouteDetailSourceView({ profile: input.profile, entries: input.entries });
  const entry = input.entries.find(
    (row) => row.source === input.profile.sourceKind && row.id === input.profile.sourceId,
  );
  const configText = source.missing ? undefined : entry?.provider?.configText;
  const caps = readCreateRouteCapabilities(configText);
  const surfaces = listLocalRouteSurfacesFromConfig(configText, {
    targetAgentId: input.profile.targetAgentId,
    ruleId: input.profile.ruleId,
  });
  const applied = appliedTargetsFromProfiles(input.siblingProfiles, input.profile);
  const enabledTargets = new Set(caps.endpoints.map((row) => row.target));
  const hasDeclaredEndpoints = caps.endpoints.length > 0;
  const host = input.host?.trim() || ROUTE_ENDPOINT_HOST;
  const port = typeof input.port === 'number' && input.port > 0 ? input.port : null;

  const rows = graphTargetsToEmit(hasDeclaredEndpoints, surfaces).map((agent): RouteGraphRow => {
    const surface = surfaces.find((row) => row.target === agent)
      ?? surfaceForCreateRouteTarget(agent);
    const upstreamBaseUrl = upstreamBaseFor({
      missing: source.missing,
      endpoints: caps.endpoints,
      target: agent,
      sourceBaseUrl: source.baseUrl,
    });
    const upstreamChannel: UpstreamChannel = source.missing
      ? 'unknown'
      : upstreamBaseUrl.trim()
        ? detectUpstreamChannelFromUrl(upstreamBaseUrl)
        : source.channel;
    const hop = hopForTestable(surface.endpointId, upstreamChannel);
    const upstreamPath = upstreamPathForChannel(upstreamChannel);
    return {
      agent,
      localPath: surface.path,
      localEndpointId: surface.endpointId,
      localUrl: routeEndpointHttpParts({
        path: surface.path,
        port,
        host,
        endpointId: surface.endpointId,
      }).href,
      upstreamBaseUrl,
      upstreamPath,
      upstreamUrl: joinUpstreamUrl(upstreamBaseUrl, upstreamPath),
      upstreamChannel,
      hop,
      link: routeGraphLinkStyle(hop),
      enabled: hasDeclaredEndpoints ? enabledTargets.has(agent) : true,
      applied: applied.has(agent),
    };
  });

  return {
    source,
    local: {
      host,
      port,
      origin: port != null ? `http://${host}:${port}` : '',
    },
    rows,
  };
}

/** Agents to advertise on the collapsed card chip row. */
export function routeGraphSupportedAgents(rows: readonly RouteGraphRow[]): CreateRouteTarget[] {
  const seen = new Set<CreateRouteTarget>();
  const agents: CreateRouteTarget[] = [];
  for (const row of rows) {
    if (!row.enabled || seen.has(row.agent)) continue;
    seen.add(row.agent);
    agents.push(row.agent);
  }
  return agents;
}

export function routeGraphLinkLabel(hop: RouteHopKind, t?: TranslateFn): string {
  if (hop === 'passthrough') {
    return graphText(t, 'routes.graph.linkPassthrough', GRAPH_COPY.linkPassthrough);
  }
  if (hop === 'convert') {
    return graphText(t, 'routes.graph.linkConvert', GRAPH_COPY.linkConvert);
  }
  return graphText(t, 'routes.graph.linkForward', GRAPH_COPY.linkForward);
}

export type RouteMappingGroup = {
  upstreamBaseUrl: string;
  upstreamPath: string;
  upstreamUrl: string;
  rows: RouteGraphRow[];
};

/** Group mapping rows that share the same upstream base + path so the UI can show the path once. */
export function groupRouteGraphRowsByUpstream(rows: readonly RouteGraphRow[]): RouteMappingGroup[] {
  const groups: RouteMappingGroup[] = [];
  const indexByKey = new Map<string, number>();
  for (const row of rows) {
    const key = `${row.upstreamBaseUrl}\0${row.upstreamPath}`;
    const existing = indexByKey.get(key);
    if (existing != null) {
      groups[existing]!.rows.push(row);
      continue;
    }
    indexByKey.set(key, groups.length);
    groups.push({
      upstreamBaseUrl: row.upstreamBaseUrl,
      upstreamPath: row.upstreamPath,
      upstreamUrl: row.upstreamUrl,
      rows: [row],
    });
  }
  return groups;
}

/** True when every row hits the same upstream endpoint (base URL + path). */
export function routeGraphSharesUpstreamEndpoint(rows: readonly RouteGraphRow[]): boolean {
  if (rows.length <= 1) return true;
  const first = rows[0];
  if (!first) return true;
  return rows.every(
    (row) => row.upstreamPath === first.upstreamPath && row.upstreamBaseUrl === first.upstreamBaseUrl,
  );
}
