import { describe, expect, it } from 'vitest';
import type { PoolAuthorizationItem } from '@/pages/bridges/route-pool-view-model';
import {
  formatPoolTimestamp,
  hasQuotaWindow,
  poolAuthorizationDetailRows,
  poolAuthorizationListChips,
} from './pool-authorization-detail';

const t = (key: string, params?: Record<string, string | number>) => {
  if (key === 'routes.pool.detail.bindingCount') return `${params?.count} 个连接`;
  if (key === 'routes.pool.detail.priorityValue') return `优先级 ${params?.n}`;
  if (key === 'routes.pool.detail.priority') return '优先级';
  if (key === 'routes.pool.detail.bindings') return '连接数量';
  if (key === 'routes.pool.detail.source') return '来源';
  if (key === 'routes.pool.page.addedHere') return '本页添加';
  if (key === 'connections.list.lastUsedAt') return '最近使用';
  return key;
};

function item(partial: Partial<PoolAuthorizationItem> = {}): PoolAuthorizationItem {
  return {
    key: 'account:grok-1',
    sourceKind: 'account',
    sourceId: 'grok-1',
    agentId: 'grok',
    title: 'Grok · OAuth',
    kind: 'oauth',
    surface: 'responses',
    addedHere: true,
    ...partial,
  };
}

describe('pool authorization detail fields', () => {
  it('hides empty quota, last used, bindings, and priority', () => {
    const rows = poolAuthorizationDetailRows(item(), t);
    expect(rows.map((row) => row.id)).toEqual(['source']);
    expect(poolAuthorizationListChips(item(), t)).toEqual([]);
  });

  it('shows last used, quota chips, bindings, and priority when present', () => {
    const row = item({
      canToggle: true,
      enabled: true,
      priority: 2,
      lastUsedAt: '2026-08-31T12:04:00Z',
      quota7dPct: 41,
      bindingCount: 2,
    });
    expect(hasQuotaWindow(row.quota7dPct)).toBe(true);
    expect(formatPoolTimestamp(row.lastUsedAt)).toMatch(/2026-08-31/);
    const chips = poolAuthorizationListChips(row, t);
    expect(chips.some((chip) => chip.includes('个连接'))).toBe(true);
    expect(chips.some((chip) => chip.includes('7d 41%'))).toBe(true);
    expect(chips.some((chip) => chip.includes('优先级 2'))).toBe(true);
    expect(poolAuthorizationDetailRows(row, t).map((entry) => entry.id)).toEqual([
      'lastUsed',
      'bindings',
      'priority',
      'source',
    ]);
  });
});
