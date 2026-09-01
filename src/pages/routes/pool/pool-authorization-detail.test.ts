import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import type { PoolAuthorizationItem } from '@/pages/bridges/route-pool-view-model';
import {
  formatPoolTimestamp,
  hasQuotaWindow,
  parseCustomModelList,
  poolAuthorizationColumnLabel,
  poolAuthorizationDetailRows,
  poolAuthorizationQuotaParts,
  poolAuthorizationVisibleColumns,
} from './pool-authorization-detail';

const t = (key: string, params?: Record<string, string | number>) => {
  if (key === 'routes.pool.detail.bindingCount') return `${params?.count} 个连接`;
  if (key === 'routes.pool.detail.priority') return '优先级';
  if (key === 'routes.pool.detail.bindings') return '连接数量';
  if (key === 'routes.pool.detail.source') return '来源';
  if (key === 'routes.pool.detail.endpointTypes') return '端点类型';
  if (key === 'routes.pool.page.addedHere') return '本页添加';
  if (key === 'routes.pool.table.login') return '登录';
  if (key === 'routes.pool.table.kind') return '类型';
  if (key === 'routes.pool.table.status') return '状态';
  if (key === 'routes.pool.detail.enabled') return '启用';
  if (key === 'routes.pool.detail.quota') return '调用窗口';
  if (key === 'connections.list.lastUsedAt') return '最近使用';
  if (key === 'routes.pool.detail.refreshTokenTail') return 'Refresh Token tail';
  if (key === 'routes.pool.surface.responsesGrok') return 'Responses · Grok';
  if (key === 'routes.pool.surface.responsesCodex') return 'Responses · Codex';
  if (key === 'routes.pool.surface.messages') return 'Messages';
  if (key === 'routes.pool.surface.chatCompletions') return 'Chat Completions';
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
    endpointKinds: ['responses_grok'],
    addedHere: true,
    ...partial,
  };
}

describe('pool authorization detail fields', () => {
  it('hides empty quota, last used, bindings, and priority', () => {
    const rows = poolAuthorizationDetailRows(item(), t);
    expect(rows.map((row) => row.id)).toEqual(['endpointTypes', 'source']);
    expect(rows.find((row) => row.id === 'endpointTypes')?.value).toContain('/v1/responses');
    expect(poolAuthorizationVisibleColumns([item()])).toEqual([
      'login',
      'kind',
      'status',
      'enabled',
    ]);
    expect(poolAuthorizationQuotaParts(item())).toEqual([]);
  });

  it('lists multiple endpoint kinds for one API key', () => {
    const rows = poolAuthorizationDetailRows(item({
      kind: 'apikey',
      agentId: 'codex',
      endpointKinds: ['responses_codex', 'chat_completions'],
    }), t);
    expect(rows.find((row) => row.id === 'endpointTypes')?.value).toContain('/v1/responses');
    expect(rows.find((row) => row.id === 'endpointTypes')?.value).toContain('/v1/chat/completions');
  });

  it('shows only the masked refresh-token tail for OAuth', () => {
    const rows = poolAuthorizationDetailRows(item({ refreshTokenTail: '**5678' }), t);
    expect(rows).toContainEqual({
      id: 'refreshTokenTail',
      label: 'Refresh Token tail',
      value: '**5678',
      mono: true,
    });
    expect(rows.map((entry) => entry.value)).not.toContain('refresh-token-secret');
  });

  it('shows last used, quota, bindings, and priority columns when present', () => {
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
    expect(poolAuthorizationQuotaParts(row)).toEqual(['7d 41%']);
    expect(poolAuthorizationVisibleColumns([row])).toEqual([
      'login',
      'kind',
      'status',
      'bindings',
      'quota',
      'lastUsed',
      'priority',
      'enabled',
    ]);
    expect(poolAuthorizationColumnLabel('bindings', t)).toBe('连接数量');
    expect(poolAuthorizationColumnLabel('quota', t)).toBe('调用窗口');
    expect(poolAuthorizationColumnLabel('lastUsed', t)).toBe('最近使用');
    expect(poolAuthorizationColumnLabel('priority', t)).toBe('优先级');
    expect(poolAuthorizationDetailRows(row, t).map((entry) => entry.id)).toEqual([
      'endpointTypes',
      'lastUsed',
      'bindings',
      'priority',
      'source',
    ]);
  });

  it('parses a custom model list', () => {
    expect(parseCustomModelList(' grok-4.5 \ngrok-4.5, grok-4.6\n')).toEqual([
      'grok-4.5',
      'grok-4.6',
    ]);
  });

  it('keeps the detail panel as a model display, not a fetch or edit form', () => {
    const source = readFileSync(
      join(dirname(fileURLToPath(import.meta.url)), 'PoolAuthorizationDetail.tsx'),
      'utf8',
    );
    expect(source).toContain('ensureSourceModelCatalog');
    expect(source).not.toContain('setSourceCustomModels');
    expect(source).not.toContain('routes.pool.detail.modelsSave');
    expect(source).not.toContain('routes.pool.page.apiModelsFetch');
  });
});
