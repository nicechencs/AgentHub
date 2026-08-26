import { describe, expect, it } from 'vitest';

import type { AgentId, UsageRecord } from '@/lib/types';

import type { UsageOverview } from '@/lib/backend/contracts/usage-types';

import {
  buildUsageDistribution,
  coerceModelFilter,
  decorateUsageDistribution,
  filterByAgent,
  filterByModel,
  filterHiddenUsageOverview,
  filterWindowUsage,
  isLocalToday,
  overviewToUsageMetrics,
  sortUsageRowsDesc,
  usageWindowBound,
} from './usageOverviewModel';

function row(
  overrides: Partial<UsageRecord> & Pick<UsageRecord, 'id' | 'timestamp' | 'agentId' | 'model'>,
): UsageRecord {
  return {
    inputTokens: 100,
    outputTokens: 20,
    cacheReadTokens: 10,
    costUsd: 1,
    sessionId: 's',
    ...overrides,
  };
}

const CATALOG = {
  claude: { name: 'Claude Code', color: '#c' },
  kimi: { name: 'Kimi', color: '#k' },
} as const;

describe('usageWindowBound', () => {
  const now = new Date(2026, 7, 23, 15, 0, 0);

  it('uses local midnight since for today and days-only otherwise', () => {
    const today = usageWindowBound('today', now);
    expect(today.days).toBe(1);
    expect(today.since).toBe(new Date(2026, 7, 23).toISOString());
    expect(usageWindowBound('24h', now)).toEqual({ days: 1 });
    expect(usageWindowBound('7d', now)).toEqual({ days: 7 });
    expect(usageWindowBound('30d', now)).toEqual({ days: 30 });
  });
});

describe('filterHiddenUsageOverview', () => {
  const overview: UsageOverview = {
    metrics: { billableInput: 1300, output: 130, cache: 200, costUsd: 2.5 },
    distribution: [
      {
        key: 'claude',
        tokens: 1300,
        costUsd: 2,
        billableInput: 1000,
        output: 100,
        cache: 200,
      },
      {
        key: 'kimi',
        tokens: 330,
        costUsd: 0.5,
        billableInput: 300,
        output: 30,
        cache: 0,
      },
    ],
    models: ['opus', 'sonnet'],
  };

  it('maps empty overview metrics to zeroed UI metrics with null cacheHitPct', () => {
    const ui = overviewToUsageMetrics({ billableInput: 0, output: 0, cache: 0, costUsd: 0 });
    expect(ui.fullInput).toBe(0);
    expect(ui.cacheHitPct).toBeNull();
  });

  it('drops omitted (hidden or uninstalled) agent slices and re-sums metrics', () => {
    const next = filterHiddenUsageOverview(overview, ['kimi'], true);
    expect(next.distribution.map((d) => d.key)).toEqual(['claude']);
    expect(next.metrics).toEqual({
      billableInput: 1000,
      output: 100,
      cache: 200,
      costUsd: 2,
    });
    const ui = overviewToUsageMetrics(next.metrics);
    expect(ui.fullInput).toBe(1200);
    expect(ui.cacheHitPct).toBe(Math.round((200 / 1200) * 100));
  });

  it('leaves model-grouped slices unchanged', () => {
    const next = filterHiddenUsageOverview(overview, ['kimi'], false);
    expect(next.distribution).toHaveLength(2);
    expect(next.metrics).toEqual(overview.metrics);
  });

  it('attaches catalog label and color', () => {
    const labeled = decorateUsageDistribution(overview.distribution, 'all', CATALOG);
    expect(labeled[0]).toMatchObject({ key: 'claude', label: 'Claude Code', color: '#c' });
    expect(labeled[1]).toMatchObject({ key: 'kimi', label: 'Kimi', color: '#k', cost: 0.5 });
  });
});

describe('filterWindowUsage', () => {
  const now = new Date(2026, 7, 23, 15, 0, 0);
  const today = new Date(2026, 7, 23, 10, 0, 0).toISOString();
  const yesterday = new Date(2026, 7, 22, 10, 0, 0).toISOString();

  it('keeps the full list for non-today ranges and drops omitted agents', () => {
    const rows = [
      row({ id: '1', timestamp: yesterday, agentId: 'claude', model: 'opus' }),
      row({ id: '2', timestamp: today, agentId: 'kimi', model: 'k2' }),
    ];
    expect(filterWindowUsage(rows, '7d', ['kimi'], now).map((r) => r.id)).toEqual(['1']);
  });

  it('narrows today to the local calendar day', () => {
    const rows = [
      row({ id: '1', timestamp: yesterday, agentId: 'claude', model: 'opus' }),
      row({ id: '2', timestamp: today, agentId: 'claude', model: 'opus' }),
    ];
    expect(filterWindowUsage(rows, 'today', [], now).map((r) => r.id)).toEqual(['2']);
    expect(filterWindowUsage(rows, '24h', [], now).map((r) => r.id)).toEqual(['1', '2']);
    expect(isLocalToday(today, now)).toBe(true);
    expect(isLocalToday(yesterday, now)).toBe(false);
  });
});

describe('model filter + shared scoped records', () => {
  const rows: UsageRecord[] = [
    row({
      id: 'c-opus',
      timestamp: '2026-08-22T10:00:00.000Z',
      agentId: 'claude',
      model: 'opus',
      inputTokens: 1000,
      outputTokens: 100,
      cacheReadTokens: 200,
      costUsd: 2,
    }),
    row({
      id: 'c-sonnet',
      timestamp: '2026-08-23T10:00:00.000Z',
      agentId: 'claude',
      model: 'sonnet',
      inputTokens: 500,
      outputTokens: 50,
      cacheReadTokens: 0,
      costUsd: 1,
    }),
    row({
      id: 'k-opus',
      timestamp: '2026-08-23T12:00:00.000Z',
      agentId: 'kimi',
      model: 'opus',
      inputTokens: 300,
      outputTokens: 30,
      cacheReadTokens: 0,
      costUsd: 0.5,
    }),
  ];

  it('coerces a stale model back to all', () => {
    expect(coerceModelFilter('opus', ['opus', 'sonnet'])).toBe('opus');
    expect(coerceModelFilter('haiku', ['opus', 'sonnet'])).toBe('all');
    expect(coerceModelFilter('all', ['opus'])).toBe('all');
  });

  it('applies agent + model to distribution and table together', () => {
    const scoped = filterByModel(filterByAgent(rows, 'all'), 'opus');

    const byAgent = buildUsageDistribution(scoped, 'all', CATALOG);
    expect(byAgent.map((d) => d.key)).toEqual(['claude', 'kimi']);
    expect(byAgent[0]?.tokens).toBe(1300);
    expect(byAgent[1]?.tokens).toBe(330);

    expect(sortUsageRowsDesc(scoped).map((r) => r.id)).toEqual(['k-opus', 'c-opus']);
  });

  it('composes time + agent + model onto one scoped list', () => {
    const now = new Date(2026, 7, 23, 15, 0, 0);
    const localRows: UsageRecord[] = [
      row({
        id: 'c-opus',
        timestamp: new Date(2026, 7, 22, 10, 0, 0).toISOString(),
        agentId: 'claude',
        model: 'opus',
        inputTokens: 1000,
        outputTokens: 100,
      }),
      row({
        id: 'c-sonnet',
        timestamp: new Date(2026, 7, 23, 10, 0, 0).toISOString(),
        agentId: 'claude',
        model: 'sonnet',
        inputTokens: 500,
        outputTokens: 50,
      }),
      row({
        id: 'k-opus',
        timestamp: new Date(2026, 7, 23, 12, 0, 0).toISOString(),
        agentId: 'kimi',
        model: 'opus',
      }),
    ];
    const windowed = filterWindowUsage(localRows, 'today', [], now);
    const scoped = filterByModel(filterByAgent(windowed, 'claude'), 'sonnet');
    expect(scoped.map((r) => r.id)).toEqual(['c-sonnet']);
  });

  it('keeps agent drill-down (distribution by model) inside the model filter', () => {
    const claudeRows = filterByAgent(rows, 'claude');
    const scoped = filterByModel(claudeRows, 'sonnet');
    const dist = buildUsageDistribution(scoped, 'claude' as AgentId, CATALOG);
    expect(dist).toEqual([
      {
        key: 'sonnet',
        label: 'sonnet',
        color: '#c',
        tokens: 550,
        cost: 1,
      },
    ]);
  });
});
