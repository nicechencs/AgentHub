import { describe, expect, it } from 'vitest';
import type { AdapterProfile, DefaultRoutePoolOverview } from '@/lib/backend/contracts/adapter';
import {
  activityHref,
  boardAttentionReason,
  boardEndpointLoginTotals,
  boardFleetSummary,
  boardLifetimeSummaryLabel,
  boardRecentSummaryLabel,
  buildBoardEndpointTypeRows,
  buildLocalEntryControl,
  buildRouteBoardStatusRows,
  mergeRecentInbound,
  parseActivityFilter,
  partitionBoardRows,
  sumRouteRequestTotals,
} from './board-view-model';

function pool(partial: Partial<DefaultRoutePoolOverview> & Pick<DefaultRoutePoolOverview, 'id'>): DefaultRoutePoolOverview {
  return {
    targetAgentId: 'codex',
    surface: 'responses',
    dialect: 'codex',
    v2Enrolled: false,
    members: [{ sourceKind: 'account', sourceId: 'acc-1', enabled: true }],
    listedModels: [],
    ...partial,
  };
}

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

describe('buildLocalEntryControl', () => {
  it('has no master action when there is no local entry', () => {
    expect(buildLocalEntryControl([], {})).toMatchObject({
      action: null,
      running: false,
      profileIds: [],
      hasEnrolledLogins: false,
    });
  });

  it('notes pool logins even when no local-bridge listener exists yet', () => {
    expect(buildLocalEntryControl([], {}, new Set(), [pool({ id: 'pool-1' })])).toMatchObject({
      action: null,
      running: false,
      startIds: [],
      hasEnrolledLogins: true,
    });
  });

  it('starts all stopped listeners and stops when any listener is up', () => {
    const stopped = buildLocalEntryControl(
      [profile({ id: 'a' }), profile({ id: 'b', targetAgentId: 'codex' })],
      {
        a: { profileId: 'a', state: 'stopped' },
        b: { profileId: 'b', state: 'error' },
      },
    );
    expect(stopped).toMatchObject({
      action: 'start',
      retry: true,
      running: false,
      startIds: ['a', 'b'],
      stopIds: [],
    });
    const mixed = buildLocalEntryControl(
      [profile({ id: 'a' }), profile({ id: 'b', targetAgentId: 'codex' })],
      {
        a: { profileId: 'a', state: 'running' },
        b: { profileId: 'b', state: 'stopped' },
      },
    );
    expect(mixed).toMatchObject({
      action: 'stop',
      running: true,
      retry: false,
      stopIds: ['a'],
      startIds: ['b'],
    });
  });

  it('omits hidden target agents', () => {
    const control = buildLocalEntryControl(
      [profile({ id: 'cursor', targetAgentId: 'cursor' })],
      { cursor: { profileId: 'cursor', state: 'stopped' } },
      new Set(['cursor']),
    );
    expect(control.action).toBeNull();
    expect(control.profileIds).toEqual([]);
  });
});

describe('buildBoardEndpointTypeRows', () => {
  it('lists four endpoint kinds and splits Responses by Codex / Grok', () => {
    const rows = buildBoardEndpointTypeRows([
      pool({
        id: 'codex-responses',
        targetAgentId: 'codex',
        surface: 'responses',
        dialect: 'codex',
        members: [
          { sourceKind: 'account', sourceId: 'oauth-1', enabled: true },
          { sourceKind: 'provider', sourceId: 'key-1', enabled: true },
          { sourceKind: 'provider', sourceId: 'key-off', enabled: false },
        ],
      }),
      pool({
        id: 'grok-responses',
        targetAgentId: 'grok',
        surface: 'responses',
        dialect: 'grok',
        members: [
          { sourceKind: 'account', sourceId: 'oauth-1', enabled: true },
          { sourceKind: 'account', sourceId: 'oauth-2', enabled: true, availability: 'isolated' },
        ],
      }),
      pool({
        id: 'claude-messages',
        targetAgentId: 'claude',
        surface: 'messages',
        dialect: 'claude',
        members: [{ sourceKind: 'account', sourceId: 'claude-oauth', enabled: true }],
      }),
    ]);
    expect(rows.map((row) => row.kind)).toEqual([
      'messages',
      'responses_codex',
      'responses_grok',
      'chat_completions',
    ]);
    expect(rows.map((row) => row.path)).toEqual([
      '/v1/messages',
      '/v1/responses',
      '/v1/responses',
      '/v1/chat/completions',
    ]);
    expect(rows.find((row) => row.kind === 'messages')).toMatchObject({
      oauthCount: 1,
      apikeyCount: 0,
    });
    expect(rows.find((row) => row.kind === 'responses_codex')).toMatchObject({
      oauthCount: 1,
      apikeyCount: 1,
    });
    expect(rows.find((row) => row.kind === 'responses_grok')).toMatchObject({
      oauthCount: 1,
      apikeyCount: 0,
    });
    expect(rows.find((row) => row.kind === 'chat_completions')).toMatchObject({
      oauthCount: 0,
      apikeyCount: 0,
    });
    expect(boardEndpointLoginTotals(rows)).toEqual({ oauth: 3, apikey: 1 });
  });

  it('omits hidden target agents from endpoint-type counts', () => {
    const rows = buildBoardEndpointTypeRows(
      [pool({
        id: 'cursor-chat',
        targetAgentId: 'cursor',
        surface: 'chat_completions',
        members: [{ sourceKind: 'provider', sourceId: 'key-1', enabled: true }],
      })],
      new Set(['cursor']),
    );
    expect(rows.find((row) => row.kind === 'chat_completions')).toMatchObject({
      oauthCount: 0,
      apikeyCount: 0,
    });
  });
});

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

  it('keeps one status card per local listener', () => {
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
    expect(rows).toHaveLength(2);
    expect(rows.map((row) => row.profileId).sort()).toEqual(['p1', 'p2']);
  });

  it('shows a connection-pool entry even without a local listener', () => {
    const rows = buildRouteBoardStatusRows(
      [],
      {},
      {},
      new Set(),
      [pool({ id: 'pool-codex', targetAgentId: 'codex' })],
    );
    expect(rows).toHaveLength(1);
    expect(rows[0].profileId).toBe('pool-codex');
    expect(rows[0].profile).toBeNull();
    expect(rows[0].memberCount).toBe(1);
    expect(rows[0].name).toMatch(/Codex/);
  });

  it('binds a running listener onto its connection-pool card', () => {
    const rows = buildRouteBoardStatusRows(
      [profile({ id: 'bridge-1', sourceId: 'acc-1', targetAgentId: 'codex', localPort: 9 })],
      { 'bridge-1': { profileId: 'bridge-1', state: 'running', port: 9, upstreamStatus: 'connected' } },
      {},
      new Set(),
      [pool({
        id: 'pool-codex',
        targetAgentId: 'codex',
        members: [{ sourceKind: 'provider', sourceId: 'acc-1', enabled: true }],
      })],
    );
    expect(rows).toHaveLength(1);
    expect(rows[0].profile?.id).toBe('bridge-1');
    expect(rows[0].state).toBe('running');
    expect(rows[0].endpoint).toBe('127.0.0.1:9');
  });

  it('omits hidden target agents from the board', () => {
    const rows = buildRouteBoardStatusRows(
      [profile({ id: 'p1', name: 'Cursor', targetAgentId: 'cursor' })],
      { p1: { profileId: 'p1', state: 'running', port: 1, upstreamStatus: 'connected' } },
      {},
      new Set(['cursor']),
    );
    expect(rows).toEqual([]);
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

  it('sums process-lifetime request counters across rows', () => {
    const rows = buildRouteBoardStatusRows(
      [profile({ id: 'a', name: 'A', sourceId: 'sa' }), profile({ id: 'b', name: 'B', sourceId: 'sb' })],
      {
        a: {
          profileId: 'a',
          state: 'running',
          port: 1,
          upstreamStatus: 'connected',
          totalRequestCount: 42,
          failedRequestCount: 3,
        },
        b: { profileId: 'b', state: 'stopped', upstreamStatus: 'stopped' },
      },
    );
    expect(sumRouteRequestTotals(rows)).toEqual({ total: 42, failed: 3 });
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
