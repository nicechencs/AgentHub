/**
 * Connections login table columns (authorization list).
 */
import type { ColumnWidthSpec } from '@/components/ui/table';
import type { TranslateFn } from '@/lib/i18n';

export type TicketWalletColumnKey = 'login' | 'kind' | 'status' | 'agent' | 'actions';

export const TICKET_WALLET_COLUMN_SPECS: ColumnWidthSpec<TicketWalletColumnKey>[] = [
  { key: 'login', defaultWidth: 240, minWidth: 160 },
  { key: 'kind', defaultWidth: 120, minWidth: 88 },
  { key: 'status', defaultWidth: 120, minWidth: 88 },
  { key: 'agent', defaultWidth: 140, minWidth: 96 },
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
    case 'agent':
      return t('connections.list.table.agent');
    case 'actions':
      return t('connections.list.table.actions');
  }
}
