import { describe, expect, it } from 'vitest';
import { createTranslator } from '@/lib/i18n';
import {
  TICKET_WALLET_COLUMN_SPECS,
  ticketWalletColumnLabel,
  type TicketWalletColumnKey,
} from './ticket-wallet-table';

describe('ticket wallet table columns', () => {
  it('keeps a stable column order', () => {
    expect(TICKET_WALLET_COLUMN_SPECS.map((spec) => spec.key)).toEqual([
      'login',
      'kind',
      'status',
      'agent',
      'actions',
    ]);
  });

  it('uses existing connection-page words for headers', () => {
    const tZh = createTranslator('zh');
    const tEn = createTranslator('en');
    const keys: TicketWalletColumnKey[] = ['login', 'kind', 'status', 'agent', 'actions'];
    expect(keys.map((key) => ticketWalletColumnLabel(key, tZh))).toEqual([
      '登录',
      '类型',
      '状态',
      'Agent',
      '操作',
    ]);
    expect(keys.map((key) => ticketWalletColumnLabel(key, tEn))).toEqual([
      'Login',
      'Type',
      'Status',
      'Agent',
      'Actions',
    ]);
  });
});
