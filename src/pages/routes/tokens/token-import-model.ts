/**
 * Eligibility for one-click「导入到 Agent」from a local-route token.
 *
 * Menu opens when the row has a key + at least one installed Agent.
 * An Agent is selectable only when this token kind is the loopback this Agent
 * actually writes (Claude←messages, Codex←responses_codex, Grok←responses_grok,
 * Kimi/DSH←chat_completions). Speaks-but-cannot-write stays visible and disabled.
 * Choosing an Agent opens Connections 「添加 API Key」 with the current URL + key.
 */
import { isAgentHidden, visibleInstalledIds } from '@/lib/agent-visibility';
import {
  buildConnectionsGuideUrl,
  type ConnectApiKeyDraft,
} from '@/lib/connect-flow/connect-intent';
import type { TranslateFn } from '@/lib/i18n';
import {
  localEndpointKindForTargetAgent,
  localEndpointSurface,
  ROUTE_ENDPOINT_HOST,
  routeEndpointHttpParts,
  type LocalEndpointKind,
  type RouteEndpointId,
} from '@/lib/route-endpoints';
import type { AdapterProfile } from '@/lib/backend/contracts/adapter';
import type { AgentKey, AgentStatus } from '@/lib/types';
import { agentConversationSurfaces } from '@/pages/agents/agent-detail-model';
import { tokenListenPort, type LocalTokenRow } from './tokens-model';

export type TokenImportAgentRef = {
  id: AgentKey;
  /** Display name for menus; callers may pass catalog name or id. */
  name: string;
};

export type TokenImportAgentChoice = TokenImportAgentRef & {
  enabled: boolean;
  /** Short per-row hint when this Agent cannot take the token. */
  reason: string | null;
};

/** Wire surface this token authenticates on. */
export function tokenImportSurface(kind: LocalEndpointKind): RouteEndpointId {
  return localEndpointSurface(kind);
}

/** True when the Agent speaks the token's conversation surface. */
export function agentMatchesTokenSurface(
  agentId: string,
  kind: LocalEndpointKind,
): boolean {
  const surface = tokenImportSurface(kind);
  return agentConversationSurfaces(agentId).includes(surface);
}

/**
 * Loopback writer kind for Agents that receive a generated local-gateway provider.
 * Null when bind/switch cannot write this token into the Agent.
 */
export function agentWritesLocalTokenKind(agentId: string): LocalEndpointKind | null {
  if (
    agentId === 'claude'
    || agentId === 'codex'
    || agentId === 'grok'
    || agentId === 'kimi'
    || agentId === 'dsh'
  ) {
    return localEndpointKindForTargetAgent(agentId);
  }
  return null;
}

/** True when importing this token writes the matching loopback into the Agent. */
export function agentCanReceiveTokenImport(
  agentId: string,
  kind: LocalEndpointKind,
): boolean {
  return agentWritesLocalTokenKind(agentId) === kind;
}

/**
 * Installed, not hidden, and able to receive this token's loopback.
 * Order follows `installedIds` when provided (catalog / stored order).
 */
export function eligibleAgentsForTokenImport(input: {
  kind: LocalEndpointKind;
  /** Prefer explicit installed+visible ids from useInstalledAgents. */
  installedIds?: readonly string[];
  /** Fallback when only raw statuses are available. */
  statuses?: ReadonlyArray<Pick<AgentStatus, 'agentId' | 'installed' | 'hidden'>>;
  /** Optional name lookup; missing names fall back to id. */
  agentName?: (agentId: string) => string;
}): TokenImportAgentRef[] {
  const ids = input.installedIds
    ?? (input.statuses ? visibleInstalledIds(input.statuses) : []);
  const nameOf = input.agentName ?? ((id: string) => id);
  const out: TokenImportAgentRef[] = [];
  for (const id of ids) {
    if (!agentCanReceiveTokenImport(id, input.kind)) continue;
    out.push({ id: id as AgentKey, name: nameOf(id) || id });
  }
  return out;
}

/** Same visibility filter as visibleInstalledIds, for a single status row. */
export function isTokenImportAgentVisible(
  status: Pick<AgentStatus, 'installed' | 'hidden'> | null | undefined,
): boolean {
  return Boolean(status?.installed) && !isAgentHidden(status);
}

export function tokenImportAgentChoice(
  kind: LocalEndpointKind,
  agent: TokenImportAgentRef,
  t?: TranslateFn,
): TokenImportAgentChoice {
  if (agentCanReceiveTokenImport(agent.id, kind)) {
    return { ...agent, enabled: true, reason: null };
  }
  if (agentWritesLocalTokenKind(agent.id) == null) {
    return {
      ...agent,
      enabled: false,
      reason: t ? t('routes.tokens.importCannotWrite') : '还不能写入',
    };
  }
  return {
    ...agent,
    enabled: false,
    reason: t ? t('routes.tokens.importEndpointMismatch') : '端点不匹配',
  };
}

export type TokenImportGate = {
  enabled: boolean;
  /** Short hint when the control cannot open; null when the menu can open. */
  reason: string | null;
  agents: TokenImportAgentChoice[];
};

/**
 * Whether「导入到 Agent」can open a menu for this row.
 * The menu lists every installed Agent; items that cannot receive this token
 * stay visible and disabled with a short reason.
 */
export function tokenImportGate(
  row: Pick<LocalTokenRow, 'kind' | 'token' | 'unavailable'>,
  agents: readonly TokenImportAgentRef[],
  t?: TranslateFn,
): TokenImportGate {
  const choices = agents.map((agent) => tokenImportAgentChoice(row.kind, agent, t));
  if (row.unavailable) {
    return {
      enabled: false,
      reason: t ? t('routes.runtime.unavailable') : '状态不可用',
      agents: choices,
    };
  }
  if (!row.token?.trim()) {
    return {
      enabled: false,
      reason: t ? t('routes.tokens.importNeedKey') : '先有入口 Key 才能导入',
      agents: choices,
    };
  }
  if (agents.length === 0) {
    return {
      enabled: false,
      reason: t ? t('routes.tokens.importNeedAgent') : '先安装 Agent',
      agents: choices,
    };
  }
  return { enabled: true, reason: null, agents: choices };
}

export function tokenImportConnectionsUrl(agentId: AgentKey): string {
  return buildConnectionsGuideUrl({ agentId, intent: 'add-key' });
}

export function tokenImportApiKeyDraft(
  row: Pick<LocalTokenRow, 'kind' | 'token' | 'path' | 'endpoint' | 'listedModels'>,
  agentId: AgentKey,
): ConnectApiKeyDraft | null {
  const apiKey = row.token?.trim();
  if (!apiKey) return null;
  if (!agentCanReceiveTokenImport(agentId, row.kind)) return null;
  const parts = routeEndpointHttpParts({
    path: row.path,
    port: tokenListenPort(row.endpoint),
    host: ROUTE_ENDPOINT_HOST,
    endpointId: tokenImportSurface(row.kind),
  });
  const model = row.listedModels?.[0]?.trim() || '';
  return {
    ...(parts.portPending ? {} : { baseUrl: parts.origin }),
    apiKey,
    ...(model ? { model } : {}),
    ...(agentId === 'grok'
      ? { apiBackend: row.kind === 'chat_completions' ? 'chat_completions' : 'responses' }
      : {}),
  };
}

/** Prefer the live profile object; fall back to the row's entry in sibling list. */
export function resolveTokenImportProfile(
  profile: AdapterProfile | null | undefined,
  profileId: string | null | undefined,
  siblings?: readonly AdapterProfile[],
): AdapterProfile | null {
  if (profile) return profile;
  const id = profileId?.trim();
  if (!id || !siblings?.length) return null;
  return siblings.find((item) => item.id === id) ?? null;
}
