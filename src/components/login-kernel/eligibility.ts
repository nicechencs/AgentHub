import type { ConnectionKind } from '@/lib/connection-kind';
import type { AgentKey } from '@/lib/types';

const POOL_SHAREABLE_OAUTH_AGENTS = new Set<AgentKey>(['claude', 'codex', 'grok']);

/** Connections add-menu: Cursor has no API Key form. */
export function canAddApiKey(agentId: AgentKey): boolean {
  return agentId !== 'cursor';
}

/** True when this Agent is in the caller's official-login support list. */
export function canStartOfficialLogin(
  agentId: AgentKey,
  supportedAgentIds: readonly AgentKey[],
): boolean {
  return supportedAgentIds.includes(agentId);
}

/**
 * Whether a Connections login may appear in「从连接同步」.
 * Any API Key qualifies. Only Claude, Codex, and Grok official logins qualify.
 * A route-pool home is already a pool login and is not a sync candidate.
 */
export function canSyncConnectionToPool(input: {
  agentId: AgentKey;
  kind: ConnectionKind;
  home?: 'route_pool';
}): boolean {
  if (input.home === 'route_pool') return false;
  if (input.kind === 'apikey') return true;
  if (input.kind === 'oauth') return POOL_SHAREABLE_OAUTH_AGENTS.has(input.agentId);
  return false;
}
