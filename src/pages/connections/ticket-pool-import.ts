/**
 * Connections row: enroll this login into the default connection pool.
 * Matches RouteDownstreamSurface::for_agent and poolSyncCandidates.
 */
import type { DefaultRoutePoolOverview } from '@/lib/backend/contracts/adapter';
import type { TicketView } from '@/lib/backend/contracts/ticket';
import type { TranslateFn } from '@/lib/i18n';
import type { AgentId } from '@/lib/types';
import type { TicketBindAction } from './ticket-bind-action';

export type { TicketBindAction };

export const POOL_IMPORTABLE_AGENTS = new Set<AgentId>(['claude', 'codex', 'grok', 'kimi', 'dsh']);

export function ticketPoolImportKey(
  ticket: Pick<TicketView, 'sourceKind' | 'sourceId'>,
): string {
  return `${ticket.sourceKind}:${ticket.sourceId}`;
}

export function importedSourceKeys(
  pools: readonly DefaultRoutePoolOverview[],
): Set<string> {
  return new Set(
    pools.flatMap((pool) =>
      pool.members.map((member) => `${member.sourceKind}:${member.sourceId}`),
    ),
  );
}

export function resolveTicketPoolImportAction(
  ticket: Pick<TicketView, 'agentId'>,
  state: { poolEnabled: boolean; alreadyImported: boolean },
  t?: TranslateFn,
): TicketBindAction {
  if (!state.poolEnabled) {
    return {
      disabled: true,
      reason: t ? t('routes.pool.page.disabledTitle') : '连接池还没开启',
    };
  }
  if (!POOL_IMPORTABLE_AGENTS.has(ticket.agentId)) {
    return {
      disabled: true,
      reason: t
        ? t('connections.list.importToPoolDisabled')
        : '这份登录目前不能分享至连接池',
    };
  }
  if (state.alreadyImported) {
    return {
      disabled: true,
      reason: t ? t('connections.list.importToPoolAlready') : '已在连接池',
    };
  }
  return { disabled: false };
}
