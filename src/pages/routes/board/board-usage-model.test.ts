import { afterEach, describe, expect, it } from 'vitest';
import type { GatewayUsageRow } from '@/lib/backend/contracts/usage-types';
import { localTrendBucket } from '@/lib/usage-trend';
import {
  BOARD_SURFACES,
  DEFAULT_BOARD_USAGE_FILTERS,
  buildBoardUsageEntries,
  buildGatewayDistribution,
  buildGatewayTrend,
  boardUsageWindow,
  filterGatewayUsageRows,
  gatewayRowTokens,
  profileToEntryIdMap,
  rememberBoardUsageFilters,
  availableBoardGroupBy,
  coerceBoardGroupBy,
  seriesKeyForRow,
  summarizeGatewayUsage,
} from './board-usage-model';

function row(partial: Partial<GatewayUsageRow> & Pick<GatewayUsageRow, 'requestId' | 'ts' | 'profileId'>): GatewayUsageRow {
  return {
    surface: 'responses',
    inputTokens: 10,
    outputTokens: 5,
    status: 'ok',
    ...partial,
  };
}

function profile(id: string, sourceId: string, targetAgentId = 'codex', name?: string) {
  return {
    id,
    name: name ?? id,
    route: 'local_bridge' as const,
    sourceKind: 'provider' as const,
    sourceId,
    targetAgentId,
  };
}

afterEach(() => {
  rememberBoardUsageFilters({ ...DEFAULT_BOARD_USAGE_FILTERS });
});

describe('availableBoardGroupBy', () => {
  it('keeps every grouping while filters are all-open and there are several entries', () => {
    expect(availableBoardGroupBy({
      entryId: 'all',
      surface: 'all',
      modelFilter: 'all',
      entryCount: 3,
    })).toEqual(['entry', 'model', 'surface']);
  });

  it('drops a grouping when that dropdown already picked one value', () => {
    expect(availableBoardGroupBy({
      entryId: 'p1',
      surface: 'all',
      modelFilter: 'all',
      entryCount: 3,
    })).toEqual(['model', 'surface']);
    expect(availableBoardGroupBy({
      entryId: 'all',
      surface: 'messages',
      modelFilter: 'all',
      entryCount: 1,
    })).toEqual(['model']);
  });

  it('coerces the selected grouping onto a still-open axis', () => {
    expect(coerceBoardGroupBy('entry', ['model', 'surface'])).toBe('surface');
    expect(coerceBoardGroupBy('model', ['model', 'surface'])).toBe('model');
  });
});

describe('boardUsageWindow', () => {
  const now = new Date(2026, 7, 31, 15, 0, 0);

  it('uses local midnight for today and a rolling bound otherwise', () => {
    const today = boardUsageWindow('today', now);
    expect(today.days).toBe(1);
    expect(today.since).toBe(new Date(2026, 7, 31).toISOString());
    const day = boardUsageWindow('7d', now);
    expect(day.days).toBe(7);
    expect(new Date(day.since).getTime()).toBe(now.getTime() - 7 * 24 * 3600 * 1000);
  });
});

describe('buildBoardUsageEntries', () => {
  it('groups profiles that share a source into one local entry', () => {
    const entries = buildBoardUsageEntries([
      profile('p1', 'src', 'claude', 'Kimi'),
      profile('p2', 'src', 'codex', 'Kimi Codex'),
      profile('p3', 'other', 'grok', 'Grok'),
    ]);
    expect(entries).toHaveLength(2);
    expect(entries.find((entry) => entry.profileIds.includes('p1'))?.profileIds).toEqual(['p1', 'p2']);
    expect(entries.map((entry) => entry.name).sort()).toEqual(['Grok', 'Kimi']);
  });

  it('drops hidden target agents', () => {
    const entries = buildBoardUsageEntries(
      [profile('p1', 'src', 'cursor', 'Cursor')],
      new Set(['cursor']),
    );
    expect(entries).toEqual([]);
  });
});

describe('gateway row math', () => {
  it('never reports negative tokens', () => {
    expect(gatewayRowTokens({
      inputTokens: -4,
      outputTokens: 8,
      cachedInputTokens: -1,
      reasoningTokens: 2,
    })).toBe(10);
  });

  it('filters by local entry, surface, and model', () => {
    const rows = [
      row({ requestId: 'a', ts: 't1', profileId: 'p1', surface: 'messages', model: 'opus' }),
      row({ requestId: 'b', ts: 't2', profileId: 'p2', surface: 'responses', model: 'gpt' }),
      row({ requestId: 'c', ts: 't3', profileId: 'p1', surface: 'messages', model: 'sonnet' }),
    ];
    expect(filterGatewayUsageRows(rows, { profileIds: ['p1'] }).map((item) => item.requestId)).toEqual(['a', 'c']);
    expect(filterGatewayUsageRows(rows, { surface: 'responses' }).map((item) => item.requestId)).toEqual(['b']);
    expect(filterGatewayUsageRows(rows, { model: 'opus' }).map((item) => item.requestId)).toEqual(['a']);
  });

  it('summarizes requests, tokens, and latency', () => {
    const totals = summarizeGatewayUsage([
      row({ requestId: 'a', ts: 't1', profileId: 'p1', inputTokens: 100, outputTokens: 20, cachedInputTokens: 10, status: 'ok', latencyMs: 100, model: 'opus' }),
      row({ requestId: 'b', ts: 't2', profileId: 'p1', inputTokens: 50, outputTokens: 5, status: 'failed', latencyMs: 300, model: 'sonnet' }),
    ]);
    expect(totals.requestCount).toBe(2);
    expect(totals.failedCount).toBe(1);
    expect(totals.inputTokens).toBe(150);
    expect(totals.outputTokens).toBe(25);
    expect(totals.cachedInputTokens).toBe(10);
    expect(totals.totalTokens).toBe(185);
    expect(totals.avgLatencyMs).toBe(200);
    expect(totals.modelNames).toEqual(['opus', 'sonnet']);
  });
});

describe('gateway trend and distribution', () => {
  const now = new Date(2026, 7, 31, 12, 0, 0);

  it('buckets tokens onto surface series', () => {
    const rows = [
      row({
        requestId: 'a',
        ts: new Date(2026, 7, 31, 10, 0, 0).toISOString(),
        profileId: 'p1',
        surface: 'messages',
        inputTokens: 100,
        outputTokens: 0,
      }),
      row({
        requestId: 'b',
        ts: new Date(2026, 7, 31, 11, 0, 0).toISOString(),
        profileId: 'p1',
        surface: 'responses',
        inputTokens: 40,
        outputTokens: 10,
      }),
    ];
    const trend = buildGatewayTrend(
      rows,
      1,
      boardUsageWindow('today', now).since,
      [...BOARD_SURFACES],
      (item) => item.surface,
      now,
    );
    expect(trend.length).toBeGreaterThan(0);
    const messagesBucket = localTrendBucket(rows[0].ts, 'hour');
    const responsesBucket = localTrendBucket(rows[1].ts, 'hour');
    expect(trend.find((point) => point.date === messagesBucket)?.messages).toBe(100);
    expect(trend.find((point) => point.date === responsesBucket)?.responses).toBe(50);
  });

  it('groups distribution by local entry via profile map', () => {
    const entries = buildBoardUsageEntries([
      profile('p1', 'src', 'claude', 'Kimi'),
      profile('p2', 'src', 'codex', 'Kimi'),
    ]);
    const map = profileToEntryIdMap(entries);
    const rows = [
      row({ requestId: 'a', ts: 't', profileId: 'p1', inputTokens: 30, outputTokens: 0 }),
      row({ requestId: 'b', ts: 't', profileId: 'p2', inputTokens: 20, outputTokens: 0 }),
    ];
    expect(seriesKeyForRow(rows[0], 'entry', map)).toBe(entries[0].id);
    const dist = buildGatewayDistribution(
      rows,
      'entry',
      map,
      { [entries[0].id]: { label: 'Kimi', color: '#c' } },
    );
    expect(dist).toHaveLength(1);
    expect(dist[0]).toMatchObject({ label: 'Kimi', tokens: 50, requests: 2 });
  });

  it('groups distribution by model and surface', () => {
    const rows = [
      row({ requestId: 'a', ts: 't', profileId: 'p1', surface: 'messages', model: 'opus', inputTokens: 10, outputTokens: 0 }),
      row({ requestId: 'b', ts: 't', profileId: 'p1', surface: 'messages', model: 'opus', inputTokens: 5, outputTokens: 0 }),
      row({ requestId: 'c', ts: 't', profileId: 'p1', surface: 'chat', model: 'gpt', inputTokens: 40, outputTokens: 0 }),
    ];
    const byModel = buildGatewayDistribution(rows, 'model', new Map(), {
      opus: { label: 'opus', color: '#a' },
      gpt: { label: 'gpt', color: '#b' },
    });
    expect(byModel.map((item) => item.key)).toEqual(['gpt', 'opus']);
    const bySurface = buildGatewayDistribution(rows, 'surface', new Map(), {
      messages: { label: 'Messages', color: '#m' },
      chat: { label: 'Chat', color: '#c' },
    });
    expect(bySurface[0]).toMatchObject({ key: 'chat', tokens: 40, requests: 1 });
  });
});
