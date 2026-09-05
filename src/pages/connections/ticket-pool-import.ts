/**
 * Who can appear in the connection pool's「从连接同步」list.
 * API keys are eligible regardless of owning Agent; official Chinese logins are not.
 * Claude / Codex / Grok official logins stay eligible. Matches poolSyncCandidates.
 */
import type { ConnectionKind } from '@/lib/connection-kind';
import type { AgentKey } from '@/lib/types';

const POOL_SHAREABLE_OAUTH_AGENTS = new Set<AgentKey>(['claude', 'codex', 'grok']);

/** True when this login can appear in「从连接同步」. */
export function isPoolShareableLogin(input: {
  agentId: AgentKey;
  credentialClass?: string;
  kind?: ConnectionKind;
}): boolean {
  const credentialClass = input.credentialClass
    ?? (input.kind === 'apikey' ? 'api_key' : input.kind === 'oauth' ? 'oauth' : undefined);
  if (credentialClass === 'api_key') return true;
  if (credentialClass === 'oauth') return POOL_SHAREABLE_OAUTH_AGENTS.has(input.agentId);
  return false;
}
