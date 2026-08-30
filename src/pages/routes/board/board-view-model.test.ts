import { describe, expect, it } from 'vitest';
import type { AdapterProfile } from '@/lib/backend/contracts/adapter';
import {
  activityHref,
  boardAttentionReason,
  boardFleetSummary,
  boardLifetimeSummaryLabel,
  boardRecentSummaryLabel,
  buildRouteBoardStatusRows,
  mergeRecentInbound,
  parseActivityFilter,
  partitionBoardRows,
} from './board-view-model';

function profile(partial: Partial<AdapterProfile> & Pick<AdapterProfile, 'id'>): AdapterProfile {
  return {
    name: partial.name ?? partial.id,
    sourceKind: 'provider',
    sourceId: partial.sourceId ?? `src-${partial.id}`,
    targetAgentId: 'claude',
    route: 'local_bridge',
    mode: 'api',
    status: 'active',
    ruleId: 'rule',
    ruleVersion: '1',
    autoStart: false,
    createdAt: '2026-01-01T00:00:00Z',
    updatedAt: '2026-01-01T00:00:00Z',
    ...partial,
  };
}

describe('board-view-model', () => {
  it('orders status rows with attention and errors first', () => {
    const rows = buildRouteBoardStatusRows(
      [
        profile({ id: 'ok', name: 'Ok', sourceId: 'a' }),
        profile({ id: 'bad', name: 'Bad', sourceId: 'b' }),
      ],
      {
        ok: { profileId: 'ok', state: 'running', port: 1, upstreamStatus: 'connected' },
        bad: { profileId: 'bad', state: 'error', port: 2, upstreamStatus: 'unavailable' },
      },
    );
    expect(rows.map((row) => row.profileId)).toEqual(['bad', 'ok']);
    expect(rows[0].endpoint).toBe('127.0.0.1:2');
    expect(rows[0].needsAttention).toBe(true);
    expect(rows[0].attentionReason).toBe('error');
  });

  it('does not treat status-read failure as stopped', () => {
    const rows = buildRouteBoardStatusRows(
      [profile({ id: 'x', name: 'X' })],
      { x: { profileId: 'x', state: 'running', port: 9, upstreamStatus: 'connected' } },
      { x: new Error('poll failed') },
    );
    expect(rows[0].statusUnavailable).toBe(true);
    expect(rows[0].needsAttention).toBe(true);
    expect(rows[0].attentionReason).toBe('unavailable');
    expect(rows[0].endpoint).toBeNull();
  });

  it('groups multiple profiles that share a source into one row', () => {
    const rows = buildRouteBoardStatusRows(
      [
        profile({ id: 'p1', name: 'A', sourceId: 'same', targetAgentId: 'claude' }),
        profile({ id: 'p2', name: 'B', sourceId: 'same', targetAgentId: 'codex' }),
      ],
      {
        p1: { profileId: 'p1', state: 'running', port: 1, upstreamStatus: 'connected' },
        p2: { profileId: 'p2', state: 'stopped', upstreamStatus: 'stopped' },
      },
    );
    expect(rows).toHaveLength(1);
  });

  it('partitions attention vs rest', () => {
    const rows = buildRouteBoardStatusRows(
      [
        profile({ id: 'ok', sourceId: 'a' }),
        profile({ id: 'bad', sourceId: 'b', lastErrorCode: 'cannot_start' }),
      ],
      {
        ok: { profileId: 'ok', state: 'running', port: 1, upstreamStatus: 'connected' },
        bad: { profileId: 'bad', state: 'error', upstreamStatus: 'unavailable' },
      },
    );
    const parts = partitionBoardRows(rows);
    expect(parts.attention.map((row) => row.profileId)).toEqual(['bad']);
    expect(parts.rest.map((row) => row.profileId)).toEqual(['ok']);
  });

  it('summarizes fleet with attention count', () => {
    const rows = buildRouteBoardStatusRows(
      [
        profile({ id: 'ok', sourceId: 'a' }),
        profile({ id: 'bad', sourceId: 'b' }),
      ],
      {
        ok: { profileId: 'ok', state: 'running', port: 1, upstreamStatus: 'connected' },
        bad: { profileId: 'bad', state: 'degraded', port: 2, upstreamStatus: 'degraded' },
      },
    );
    const fleet = boardFleetSummary(rows);
    expect(fleet).toMatchObject({ total: 2, running: 2, needsAttention: 1 });
    expect(fleet?.label).toContain('需要处理');
  });

  it('merges inbound requests newest first and caps length', () => {
    const merged = mergeRecentInbound(
      [
        profile({ id: 'a', name: 'Alpha' }),
        profile({ id: 'b', name: 'Beta' }),
      ],
      {
        a: {
          profileId: 'a',
          state: 'running',
          recentInbound: [
            { at: '2026-08-12T00:00:01.000Z', method: 'POST', path: '/v1/a', status: 200, ok: true },
          ],
        },
        b: {
          profileId: 'b',
          state: 'running',
          recentInbound: [
            { at: '2026-08-12T00:00:03.000Z', method: 'GET', path: '/models', status: 500, ok: false },
            { at: '2026-08-12T00:00:02.000Z', method: 'POST', path: '/v1/b', status: 200, ok: true },
          ],
        },
      },
      2,
    );
    expect(merged).toHaveLength(2);
    expect(merged[0].path).toBe('/models');
    expect(merged[0].sourceLabel).toBe('Beta');
    expect(merged[1].path).toBe('/v1/b');
  });

  it('builds activity deep links and parses filter', () => {
    expect(activityHref({})).toBe('/routes/activity');
    expect(activityHref({ filter: 'failed' })).toBe('/routes/activity?filter=failed');
    expect(activityHref({ route: 'p1', filter: 'failed' })).toBe(
      '/routes/activity?filter=failed&route=p1',
    );
    expect(parseActivityFilter('failed')).toBe('failed');
    expect(parseActivityFilter('nope')).toBe('all');
  });

  it('labels attention reasons and recent summaries', () => {
    expect(boardAttentionReason({
      statusUnavailable: true,
      state: 'running',
      profileStatus: 'active',
    })).toBe('unavailable');
    expect(boardRecentSummaryLabel(
      { lastAt: null, failedInWindow: 0, windowSize: 0, totalRequestCount: 0, failedRequestCount: 0 },
      null,
    )).toContain('还没有请求');
    expect(boardRecentSummaryLabel(
      { lastAt: 't', failedInWindow: 2, windowSize: 5, totalRequestCount: 25, failedRequestCount: 5 },
      '3 分钟前',
    )).toContain('失败');
  });

  it('exposes process-lifetime counters and prefers lastRequestAt', () => {
    const rows = buildRouteBoardStatusRows(
      [profile({ id: 'p1', name: 'Counted' })],
      {
        p1: {
          profileId: 'p1',
          state: 'running',
          port: 9,
          upstreamStatus: 'connected',
          totalRequestCount: 42,
          failedRequestCount: 3,
          lastRequestAt: '2026-08-12T00:00:09.000Z',
          recentInbound: [
            { at: '2026-08-12T00:00:01.000Z', method: 'GET', path: '/models', status: 200, ok: true },
          ],
        },
      },
    );
    expect(rows[0].recent).toMatchObject({
      lastAt: '2026-08-12T00:00:09.000Z',
      totalRequestCount: 42,
      failedRequestCount: 3,
      windowSize: 1,
    });
    expect(boardLifetimeSummaryLabel(rows[0].recent)).toBe('共 42 次 · 失败 3 次');
    expect(boardLifetimeSummaryLabel({
      lastAt: null,
      failedInWindow: 0,
      windowSize: 0,
      totalRequestCount: 10,
      failedRequestCount: 0,
    })).toBe('共 10 次');
    expect(boardLifetimeSummaryLabel({
      lastAt: null,
      failedInWindow: 0,
      windowSize: 0,
      totalRequestCount: 0,
      failedRequestCount: 0,
    })).toBeNull();
  });
});
