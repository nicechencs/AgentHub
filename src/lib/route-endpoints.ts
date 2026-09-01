/**
 * Unified local-route downstream surfaces. Bind still targets a writer Agent
 * internally; UI for purpose=route shows these endpoints instead.
 *
 * Wire surfaces stay `messages | responses | chat_completions`. The UI further
 * splits `/v1/responses` into Codex (generic OpenAI Responses) vs Grok
 * (Grok-native Responses) via {@link LocalEndpointKind}.
 */
import type { AgentId } from '@/lib/types';
import type { TokenAgentId } from '@/styles/tokens';

export type RouteEndpointId = 'messages' | 'responses' | 'chat_completions';

/** UI endpoint kind; Responses is split by dialect. */
export type LocalEndpointKind =
  | 'messages'
  | 'responses_codex'
  | 'responses_grok'
  | 'chat_completions';

export interface RouteEndpoint {
  id: RouteEndpointId;
  path: string;
}

export interface LocalEndpointSpec {
  kind: LocalEndpointKind;
  path: string;
  /** Wire / gateway surface. */
  surface: RouteEndpointId;
  brandAgentId: TokenAgentId;
}

export const ROUTE_ENDPOINTS: readonly RouteEndpoint[] = [
  { id: 'messages', path: '/v1/messages' },
  { id: 'responses', path: '/v1/responses' },
  { id: 'chat_completions', path: '/v1/chat/completions' },
];

/** Board / tokens / pool-detail rows: four kinds, two sharing `/v1/responses`. */
export const LOCAL_ENDPOINT_KINDS: readonly LocalEndpointSpec[] = [
  { kind: 'messages', path: '/v1/messages', surface: 'messages', brandAgentId: 'claude' },
  {
    kind: 'responses_codex',
    path: '/v1/responses',
    surface: 'responses',
    brandAgentId: 'codex',
  },
  {
    kind: 'responses_grok',
    path: '/v1/responses',
    surface: 'responses',
    brandAgentId: 'grok',
  },
  {
    kind: 'chat_completions',
    path: '/v1/chat/completions',
    surface: 'chat_completions',
    brandAgentId: 'codex',
  },
];

const ENDPOINT_BY_ID: Record<RouteEndpointId, RouteEndpoint> = {
  messages: ROUTE_ENDPOINTS[0]!,
  responses: ROUTE_ENDPOINTS[1]!,
  chat_completions: ROUTE_ENDPOINTS[2]!,
};

const LOCAL_BY_KIND: Record<LocalEndpointKind, LocalEndpointSpec> = {
  messages: LOCAL_ENDPOINT_KINDS[0]!,
  responses_codex: LOCAL_ENDPOINT_KINDS[1]!,
  responses_grok: LOCAL_ENDPOINT_KINDS[2]!,
  chat_completions: LOCAL_ENDPOINT_KINDS[3]!,
};

/** Writer Agent → the loopback path that agent consumes. */
export function routeEndpointIdForTargetAgent(agentId: AgentId | string): RouteEndpointId {
  if (agentId === 'claude') return 'messages';
  if (agentId === 'codex' || agentId === 'grok') return 'responses';
  return 'chat_completions';
}

/** Writer Agent → UI endpoint kind (Codex vs Grok Responses split). */
export function localEndpointKindForTargetAgent(agentId: AgentId | string): LocalEndpointKind {
  if (agentId === 'claude') return 'messages';
  if (agentId === 'codex') return 'responses_codex';
  if (agentId === 'grok') return 'responses_grok';
  return 'chat_completions';
}

/** Pool surface + dialect → UI endpoint kind. */
export function localEndpointKindFromPool(input: {
  surface: RouteEndpointId | string;
  dialect?: string | null;
  targetAgentId?: string | null;
}): LocalEndpointKind | null {
  if (input.surface === 'messages') return 'messages';
  if (input.surface === 'chat_completions') return 'chat_completions';
  if (input.surface !== 'responses') return null;
  const dialect = input.dialect?.trim() || input.targetAgentId?.trim() || '';
  if (dialect === 'grok') return 'responses_grok';
  return 'responses_codex';
}

export function localEndpointSpec(kind: LocalEndpointKind): LocalEndpointSpec {
  return LOCAL_BY_KIND[kind];
}

export function localEndpointPath(kind: LocalEndpointKind): string {
  return LOCAL_BY_KIND[kind].path;
}

export function localEndpointSurface(kind: LocalEndpointKind): RouteEndpointId {
  return LOCAL_BY_KIND[kind].surface;
}

export function localEndpointBrandAgentId(kind: LocalEndpointKind): TokenAgentId {
  return LOCAL_BY_KIND[kind].brandAgentId;
}

export function isLocalEndpointKind(value: string): value is LocalEndpointKind {
  return Object.prototype.hasOwnProperty.call(LOCAL_BY_KIND, value);
}

/**
 * Prefer the capability-matrix rule when present so Grok-as-writer
 * (Responses) is not confused with chat-completions agents.
 */
export function routeEndpointIdForRuleId(ruleId: string | null | undefined): RouteEndpointId | null {
  if (!ruleId) return null;
  if (ruleId.includes('-to-claude')) return 'messages';
  if (ruleId.includes('-to-codex') || ruleId.includes('-to-grok')) return 'responses';
  if (ruleId.includes('-to-kimi') || ruleId.includes('-to-dsh')) return 'chat_completions';
  return null;
}

export function routeEndpointIdForBinding(input: {
  agentId: AgentId | string;
  ruleId?: string | null;
}): RouteEndpointId {
  return routeEndpointIdForRuleId(input.ruleId) ?? routeEndpointIdForTargetAgent(input.agentId);
}

export function routeEndpointById(id: RouteEndpointId): RouteEndpoint {
  return ENDPOINT_BY_ID[id];
}

export function routeEndpointPath(id: RouteEndpointId): string {
  return ENDPOINT_BY_ID[id].path;
}

export function routeEndpointPathForBinding(input: {
  agentId: AgentId | string;
  ruleId?: string | null;
}): string {
  return routeEndpointPath(routeEndpointIdForBinding(input));
}

const SURFACE_LABEL: Record<RouteEndpointId, string> = {
  messages: 'Claude',
  responses: 'Codex',
  chat_completions: 'Kimi',
};

/** Client-facing local-route name, e.g. `Claude`. */
export function routeEndpointSurfaceLabel(id: RouteEndpointId): string {
  return SURFACE_LABEL[id];
}

export const ROUTE_ENDPOINT_HOST = '127.0.0.1';
export const ROUTE_ENDPOINT_PENDING_PORT = '{port}';

/**
 * Which Agent token a surface reuses. Not a second palette —
 * colors still come from `AGENT_COLORS` / `--agent-*`.
 * Messages → Claude; OpenAI-family paths → Codex (Grok's token is black).
 */
export function routeEndpointBrandAgentId(id: RouteEndpointId): TokenAgentId {
  if (id === 'messages') return 'claude';
  return 'codex';
}

export type RouteEndpointHttpParts = {
  host: string;
  portLabel: string;
  portPending: boolean;
  origin: string;
  path: string;
  href: string | null;
  display: string;
  endpointId: RouteEndpointId;
  brandAgentId: TokenAgentId;
};

export function routeEndpointHttpParts(input: {
  path: string;
  port?: number | null;
  host?: string;
  endpointId?: RouteEndpointId;
}): RouteEndpointHttpParts {
  const host = input.host?.trim() || ROUTE_ENDPOINT_HOST;
  const port = typeof input.port === 'number' && input.port > 0 ? input.port : null;
  const portLabel = port != null ? String(port) : ROUTE_ENDPOINT_PENDING_PORT;
  const origin = `http://${host}:${portLabel}`;
  const path = input.path.startsWith('/') ? input.path : `/${input.path}`;
  const endpointId = input.endpointId
    ?? (path === '/v1/messages'
      ? 'messages'
      : path === '/v1/responses'
        ? 'responses'
        : 'chat_completions');
  return {
    host,
    portLabel,
    portPending: port == null,
    origin,
    path,
    href: port != null ? `http://${host}:${port}${path}` : null,
    display: `${origin}${path}`,
    endpointId,
    brandAgentId: routeEndpointBrandAgentId(endpointId),
  };
}

export function formatRouteEndpointHttpUrl(input: {
  path: string;
  port?: number | null;
  host?: string;
}): string {
  return routeEndpointHttpParts(input).display;
}
