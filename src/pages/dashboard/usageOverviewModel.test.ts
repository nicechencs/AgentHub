import { describe, expect, it } from 'vitest';

import type { AgentId, UsageRecord } from '@/lib/types';

import {
  buildUsageDistribution,
  buildUsageTrend,
  coerceModelFilter,
  computeUsageMetrics,
  filterByAgent,
  filterByModel,
  filterWindowUsage,
  isLocalToday,
  modelsFromRecords,
  sortUsageRowsDesc,
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

describe('filterWindowUsage', () => {
  const now = new Date(2026, 7, 23, 15, 0, 0);
  const today = new Date(2026, 7, 23, 10, 0, 0).toISOString();
  const yesterday = new Date(2026, 7, 22, 10, 0, 0).toISOString();

  it('keeps the full list for non-today ranges and drops hidden agents', () => {
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

  it('lists distinct models from the current window, sorted', () => {
    expect(modelsFromRecords(rows)).toEqual(['opus', 'sonnet']);
    expect(modelsFromRecords(filterByModel(rows, 'opus'))).toEqual(['opus']);
  });

  it('coerces a stale model back to all', () => {
    expect(coerceModelFilter('opus', ['opus', 'sonnet'])).toBe('opus');
    expect(coerceModelFilter('haiku', ['opus', 'sonnet'])).toBe('all');
    expect(coerceModelFilter('all', ['opus'])).toBe('all');
  });

  it('scopes the model list to the selected agent in the time window', () => {
    const claudeRows = filterByAgent(rows, 'claude');
    expect(modelsFromRecords(claudeRows)).toEqual(['opus', 'sonnet']);
    expect(modelsFromRecords(filterByAgent(rows, 'kimi'))).toEqual(['opus']);
  });

  it('applies agent + model to metrics, trend, distribution, and table together', () => {
    const scoped = filterByModel(filterByAgent(rows, 'all'), 'opus');
    const metrics = computeUsageMetrics(scoped);
    expect(metrics.billableInput).toBe(1300);
    expect(metrics.output).toBe(130);
    expect(metrics.cost).toBe(2.5);
    expect(metrics.cacheHitPct).toBe(Math.round((200 / 1500) * 100));

    expect(buildUsageTrend(scoped)).toEqual([
      { date: '2026-08-22', claude: 1100 },
      { date: '2026-08-23', kimi: 330 },
    ]);

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
    expect(computeUsageMetrics(scoped).billableInput).toBe(500);
    expect(buildUsageTrend(scoped)[0]?.claude).toBe(550);
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
    expect(buildUsageTrend(scoped)).toEqual([{ date: '2026-08-23', claude: 550 }]);
  });

  it('matches backend trend formula (input + output, not cache)', () => {
    const scoped = filterByModel(rows, 'opus');
    const claude = scoped.find((r) => r.agentId === 'claude')!;
    expect(buildUsageTrend([claude])[0]?.claude).toBe(
      claude.inputTokens + claude.outputTokens,
    );
  });
});
