/**
 * Connections row: enroll this login into the default connection pool.
 * API keys are eligible regardless of owning Agent; official Chinese logins are not.
 * Claude / Codex / Grok official logins stay eligible. Matches poolSyncCandidates.
 */
import type { DefaultRoutePoolOverview } from '@/lib/backend/contracts/adapter';
import type { TicketView } from '@/lib/backend/contracts/ticket';
import type { ConnectionKind } from '@/lib/connection-kind';
import type { TranslateFn } from '@/lib/i18n';
import type { AgentKey } from '@/lib/types';
import type { TicketBindAction } from './ticket-bind-action';

export type { TicketBindAction };

const POOL_SHAREABLE_OAUTH_AGENTS = new Set<AgentKey>(['claude', 'codex', 'grok']);

/** True when this login can appear in「从连接同步」/「分享至连接池」. */
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

export function ticketPoolImportMenuState(
  action: TicketBindAction,
  importingId: string | null,
  ticketId: string,
  t?: TranslateFn,
): { disabled: boolean; reason?: string; busy: boolean } {
  const importing = importingId === ticketId;
  const importBusy = importingId !== null;
  const disabled = action.disabled || importBusy;
  const reason = importing
    ? (t ? t('connections.list.importingToPool') : '分享中…')
    : importBusy
      ? (t ? t('connections.list.importToPoolBusy') : '正在分享其他登录')
      : action.disabled
        ? action.reason
        : undefined;
  return { disabled, reason, busy: importing };
}

export function resolveTicketPoolImportAction(
  ticket: Pick<TicketView, 'agentId' | 'credentialClass'>,
  state: { poolEnabled: boolean; alreadyImported: boolean },
  t?: TranslateFn,
): TicketBindAction {
  if (!state.poolEnabled) {
    return {
      disabled: true,
      reason: t ? t('routes.pool.page.disabledTitle') : '连接池还没开启',
    };
  }
  if (!isPoolShareableLogin(ticket)) {
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
