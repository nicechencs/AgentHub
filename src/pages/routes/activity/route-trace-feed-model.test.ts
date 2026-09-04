import { describe, expect, it } from 'vitest';
import { buildRouteTraceFeed, decorateRouteTraceRows, mergeRecentRouteTraces, UNAUTHENTICATED_TRACE_PROFILE_ID } from './route-trace-feed-model';
import type { AdapterBridgeRuntimeStatus, AdapterProfile } from '@/lib/backend/contracts/adapter';

const profiles: Pick<AdapterProfile, 'id' | 'name' | 'route' | 'targetAgentId'>[] = [
  { id: 'route-a', name: 'Route A', route: 'local_bridge', targetAgentId: 'cursor' },
];

const trace = {
  traceVersion: 2,
  requestId: 'req-1',
  at: '2026-01-01T00:00:00.000Z',
  method: 'POST',
  path: '/v1/messages',
  httpStatus: 401,
  ok: false,
  localAuth: { status: 'failed' as const, code: 'invalid_api_key' },
  pool: { status: 'skipped' as const },
  conversion: { status: 'skipped' as const, path: '' },
  upstreamAuth: { status: 'skipped' as const },
  upstream: { status: 'skipped' as const },
  failureStage: 'local_auth' as const,
};

describe('route-trace-feed-model', () => {
  it('merges traces across profiles newest first', () => {
    const statuses: Record<string, AdapterBridgeRuntimeStatus> = {
      'route-a': {
        profileId: 'route-a',
        state: 'running',
        recentRouteTraces: [trace],
      },
    };
    const rows = mergeRecentRouteTraces(profiles, statuses, 10);
    expect(rows).toHaveLength(1);
    expect(rows[0]?.profileId).toBe('route-a');
    expect(rows[0]?.sourceLabel).toBe('Route A');
  });

  it('falls back to legacy inbound rows when traces are missing', () => {
    const statuses: Record<string, AdapterBridgeRuntimeStatus> = {
      'route-a': {
        profileId: 'route-a',
        state: 'running',
        recentInbound: [
          {
            at: '2026-01-01T00:00:00.000Z',
            method: 'POST',
            path: '/v1/messages',
            status: 200,
            ok: true,
          },
        ],
      },
    };
    const rows = mergeRecentRouteTraces(profiles, statuses, 10);
    expect(rows).toHaveLength(1);
    expect(rows[0]?.legacySummary).toBe(true);
  });

  it('merges unauthenticated traces', () => {
    const rows = mergeRecentRouteTraces(profiles, {}, 10, {
      unauthenticatedTraces: [trace],
      unauthenticatedSourceLabel: '未绑定路由',
    });
    expect(rows).toHaveLength(1);
    expect(rows[0]?.profileId).toBe(UNAUTHENTICATED_TRACE_PROFILE_ID);
    expect(rows[0]?.sourceLabel).toBe('未绑定路由');
    expect(rows[0]?.unauthenticated).toBe(true);
  });

  it('filters failed traces', () => {
    const okTrace = { ...trace, requestId: 'req-2', ok: true, httpStatus: 200, failureStage: null };
    const statuses: Record<string, AdapterBridgeRuntimeStatus> = {
      'route-a': {
        profileId: 'route-a',
        state: 'running',
        recentRouteTraces: [trace, okTrace],
      },
    };
    const failed = buildRouteTraceFeed(profiles, statuses, 'failed', 10);
    expect(failed).toHaveLength(1);
    expect(failed[0]?.requestId).toBe('req-1');
  });

  it('labels queried traces with the route name', () => {
    const rows = decorateRouteTraceRows(
      [{ ...trace, profileId: 'route-a' }],
      profiles,
      '未绑定路由',
    );
    expect(rows[0]?.sourceLabel).toBe('Route A');
    expect(rows[0]?.unauthenticated).toBe(false);
  });
});
