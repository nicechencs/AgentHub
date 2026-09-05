/**
 * Connections login table columns (authorization list).
 */
import type { ColumnWidthSpec } from '@/components/ui/table';
import type { TranslateFn } from '@/lib/i18n';
import { fmtTokens } from '@/lib/utils';
import {
  hasOfficialQuotaWindow,
  type TicketDetailExtras,
} from './ticket-card-detail';

export type TicketWalletColumnKey =
  | 'login'
  | 'kind'
  | 'status'
  | 'lastUsed'
  | 'usage'
  | 'agent'
  | 'actions';

export const TICKET_WALLET_COLUMN_SPECS: ColumnWidthSpec<TicketWalletColumnKey>[] = [
  { key: 'login', defaultWidth: 200, minWidth: 148 },
  { key: 'kind', defaultWidth: 104, minWidth: 80 },
  { key: 'status', defaultWidth: 104, minWidth: 80 },
  { key: 'lastUsed', defaultWidth: 148, minWidth: 120 },
  { key: 'usage', defaultWidth: 168, minWidth: 128 },
  { key: 'agent', defaultWidth: 120, minWidth: 88 },
  { key: 'actions', defaultWidth: 176, minWidth: 128 },
];

export function ticketWalletColumnLabel(
  key: TicketWalletColumnKey,
  t: TranslateFn,
): string {
  switch (key) {
    case 'login':
      return t('connections.list.table.login');
    case 'kind':
      return t('connections.list.table.kind');
    case 'status':
      return t('connections.list.table.status');
    case 'lastUsed':
      return t('connections.list.lastUsedAt');
    case 'usage':
      return t('connections.list.usage');
    case 'agent':
      return t('connections.list.table.agent');
    case 'actions':
      return t('connections.list.table.actions');
  }
}

/** Compact 7d / 5h percents for the connections table. Empty when unknown. */
export function ticketWalletQuotaParts(
  extras?: Pick<TicketDetailExtras, 'quota5hPct' | 'quota7dPct'> | null,
): string[] {
  const parts: string[] = [];
  const pct7d = extras?.quota7dPct;
  const pct5h = extras?.quota5hPct;
  if (hasOfficialQuotaWindow(pct7d)) parts.push(`7d ${pct7d}%`);
  if (hasOfficialQuotaWindow(pct5h)) parts.push(`5h ${pct5h}%`);
  return parts;
}

export function ticketWalletTokenUsageText(
  extras?: Pick<TicketDetailExtras, 'tokenInput' | 'tokenOutput'> | null,
  t?: TranslateFn,
): string | null {
  const input = extras?.tokenInput;
  const output = extras?.tokenOutput;
  const hasInput = typeof input === 'number' && Number.isFinite(input);
  const hasOutput = typeof output === 'number' && Number.isFinite(output);
  if (!hasInput && !hasOutput) return null;
  const inText = fmtTokens(hasInput ? input : 0);
  const outText = fmtTokens(hasOutput ? output : 0);
  if (t) return t('connections.list.tokenUsage', { in: inText, out: outText });
  return `${inText} / ${outText}`;
}

/** Percents when the official window exists; otherwise token totals. */
export function ticketWalletUsageParts(
  extras?: Pick<
    TicketDetailExtras,
    'quota5hPct' | 'quota7dPct' | 'tokenInput' | 'tokenOutput'
  > | null,
  t?: TranslateFn,
): string[] {
  const quota = ticketWalletQuotaParts(extras);
  if (quota.length > 0) return quota;
  const tokens = ticketWalletTokenUsageText(extras, t);
  return tokens ? [tokens] : [];
}
