import { describe, expect, it } from 'vitest';
import { createTranslator } from '@/lib/i18n';
import { formatDetailTimestamp } from './ticket-card-detail';
import {
  TICKET_WALLET_COLUMN_SPECS,
  ticketWalletColumnLabel,
  ticketWalletQuotaParts,
  ticketWalletUsageParts,
  type TicketWalletColumnKey,
} from './ticket-wallet-table';

describe('ticket wallet table columns', () => {
  it('keeps a stable column order', () => {
    expect(TICKET_WALLET_COLUMN_SPECS.map((spec) => spec.key)).toEqual([
      'login',
      'kind',
      'status',
      'lastUsed',
      'usage',
      'agent',
      'actions',
    ]);
  });

  it('uses existing connection-page words for headers', () => {
    const tZh = createTranslator('zh');
    const tEn = createTranslator('en');
    const keys: TicketWalletColumnKey[] = [
      'login',
      'kind',
      'status',
      'lastUsed',
      'usage',
      'agent',
      'actions',
    ];
    expect(keys.map((key) => ticketWalletColumnLabel(key, tZh))).toEqual([
      '登录',
      '类型',
      '状态',
      '最近使用',
      '用量',
      'Agent',
      '操作',
    ]);
    expect(keys.map((key) => ticketWalletColumnLabel(key, tEn))).toEqual([
      'Login',
      'Type',
      'Status',
      'Last used',
      'Usage',
      'Agent',
      'Actions',
    ]);
  });

  it('formats last used and compact 7d/5h usage for the list', () => {
    expect(formatDetailTimestamp('2026-08-28T08:00:00.000Z')).toMatch(
      /^\d{4}-\d{2}-\d{2} \d{2}:\d{2}$/,
    );
    expect(ticketWalletQuotaParts(null)).toEqual([]);
    expect(ticketWalletQuotaParts({ quota7dPct: 22 })).toEqual(['7d 22%']);
    expect(ticketWalletQuotaParts({ quota7dPct: 89, quota5hPct: 12 })).toEqual([
      '7d 89%',
      '5h 12%',
    ]);
    expect(ticketWalletUsageParts({ quota7dPct: 22, tokenInput: 100, tokenOutput: 9 })).toEqual([
      '7d 22%',
    ]);
    expect(ticketWalletUsageParts({ tokenInput: 12, tokenOutput: 3 })).toEqual(['12 / 3']);
    expect(ticketWalletUsageParts({ tokenInput: 1_234_567, tokenOutput: 89_000 })).toEqual([
      '1.2M / 89.0K',
    ]);
    expect(ticketWalletUsageParts(null)).toEqual([]);
  });
});
