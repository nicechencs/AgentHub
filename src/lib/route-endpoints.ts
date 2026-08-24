/**
 * Unified local-route downstream surfaces. Bind still targets a writer Agent
 * internally; UI for purpose=route shows these three endpoints instead.
 */
import type { AgentId } from '@/lib/types';

export type RouteEndpointId = 'messages' | 'responses' | 'chat_completions';

export interface RouteEndpoint {
  id: RouteEndpointId;
  path: string;
}

export const ROUTE_ENDPOINTS: readonly RouteEndpoint[] = [
  { id: 'messages', path: '/v1/messages' },
  { id: 'responses', path: '/v1/responses' },
  { id: 'chat_completions', path: '/v1/chat/completions' },
];

const ENDPOINT_BY_ID: Record<RouteEndpointId, RouteEndpoint> = {
  messages: ROUTE_ENDPOINTS[0]!,
  responses: ROUTE_ENDPOINTS[1]!,
  chat_completions: ROUTE_ENDPOINTS[2]!,
};

/** Writer Agent → the loopback path that agent consumes. */
export function routeEndpointIdForTargetAgent(agentId: AgentId | string): RouteEndpointId {
  if (agentId === 'claude') return 'messages';
  if (agentId === 'codex' || agentId === 'grok') return 'responses';
  return 'chat_completions';
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
  messages: 'Messages /v1/messages',
  responses: 'Responses /v1/responses',
  chat_completions: 'Chat Completions /v1/chat/completions',
};

/** Agent-facing local-route surface, e.g. `Messages /v1/messages`. */
export function routeEndpointSurfaceLabel(id: RouteEndpointId): string {
  return SURFACE_LABEL[id];
}

export const ROUTE_ENDPOINT_HOST = '127.0.0.1';
export const ROUTE_ENDPOINT_PENDING_PORT = '{port}';

/**
 * Brand color for each unified surface. Chat Completions uses Kimi (not Grok)
 * so the path stays visible — Grok's token is black.
 */
export function routeEndpointBrandAgentId(id: RouteEndpointId): AgentId {
  if (id === 'messages') return 'claude';
  if (id === 'responses') return 'codex';
  return 'kimi';
}

export type RouteEndpointHttpParts = {
  host: string;
  portLabel: string;
  portPending: boolean;
  origin: string;
  path: string;
  href: string | null;
  display: string;
  brandAgentId: AgentId;
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
