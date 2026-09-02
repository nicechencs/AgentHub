/**
 * Pure eligibility for one-click「导入到 Agent」from a local-route token.
 * Surface truth: agentConversationSurfaces ∩ localEndpointSurface(kind).
 * Visibility: installed && !hidden (same rule as useInstalledAgents / visibleInstalledIds).
 */
import { isAgentHidden, visibleInstalledIds } from '@/lib/agent-visibility';
import type { TranslateFn } from '@/lib/i18n';
import {
  localEndpointSurface,
  type LocalEndpointKind,
  type RouteEndpointId,
} from '@/lib/route-endpoints';
import type { AgentId, AgentStatus } from '@/lib/types';
import { agentConversationSurfaces } from '@/pages/agents/agent-detail-model';
import type { LocalTokenRow } from './tokens-model';

export type TokenImportAgentRef = {
  id: AgentId;
  /** Display name for menus; callers may pass catalog name or id. */
  name: string;
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
 * Installed, not hidden, and surface-matched — menu candidates only.
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
    if (!agentMatchesTokenSurface(id, input.kind)) continue;
    out.push({ id: id as AgentId, name: nameOf(id) || id });
  }
  return out;
}

/** Same visibility filter as visibleInstalledIds, for a single status row. */
export function isTokenImportAgentVisible(
  status: Pick<AgentStatus, 'installed' | 'hidden'> | null | undefined,
): boolean {
  return Boolean(status?.installed) && !isAgentHidden(status);
}

export type TokenImportGate = {
  enabled: boolean;
  /** Short hint when disabled; null when the menu can open. */
  reason: string | null;
  agents: TokenImportAgentRef[];
};

/**
 * Whether「导入到 Agent」can open a menu for this row.
 * No empty menu: disable + hint when nobody is eligible.
 */
export function tokenImportGate(
  row: Pick<LocalTokenRow, 'kind' | 'token' | 'profileId' | 'unavailable'>,
  agents: readonly TokenImportAgentRef[],
  t?: TranslateFn,
): TokenImportGate {
  const eligible = agents.filter((agent) => agentMatchesTokenSurface(agent.id, row.kind));
  if (row.unavailable) {
    return {
      enabled: false,
      reason: t ? t('routes.runtime.unavailable') : '状态不可用',
      agents: eligible,
    };
  }
  if (!row.token?.trim()) {
    return {
      enabled: false,
      reason: t ? t('routes.tokens.importNeedKey') : '先有入口 Key 才能导入',
      agents: eligible,
    };
  }
  if (!row.profileId?.trim()) {
    return {
      enabled: false,
      reason: t ? t('routes.tokens.importNeedEntry') : '本机入口还没就绪',
      agents: eligible,
    };
  }
  if (eligible.length === 0) {
    return {
      enabled: false,
      reason: t
        ? t('routes.tokens.importNoneEligible')
        : '没有已安装且匹配此端点的 Agent',
      agents: eligible,
    };
  }
  return { enabled: true, reason: null, agents: eligible };
}
