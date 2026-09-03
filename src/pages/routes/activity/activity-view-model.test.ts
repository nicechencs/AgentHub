import { describe, expect, it } from 'vitest';
import { resolveActivityPageSnapshot } from './activity-view-model';
import type { AdapterProfile } from '@/lib/backend/contracts/adapter';

const profile: AdapterProfile = {
  id: 'route-a',
  name: 'Route A',
  route: 'local_bridge',
  targetAgentId: 'claude',
  sourceKind: 'account',
  sourceId: 'acc-1',
  mode: 'api',
  status: 'active',
  ruleId: 'rule',
  ruleVersion: '1',
  autoStart: false,
  createdAt: '',
  updatedAt: '',
};

describe('resolveActivityPageSnapshot', () => {
  it('uses activity-specific noLogins state instead of board empty copy', () => {
    const snapshot = resolveActivityPageSnapshot({
      profiles: [],
      bridgeStatuses: {},
      pools: [],
      filter: 'all',
      profileState: 'ready',
      loading: false,
    });
    expect(snapshot.kind).toBe('noLogins');
  });

  it('detects pool logins without local routes', () => {
    const snapshot = resolveActivityPageSnapshot({
      profiles: [],
      bridgeStatuses: {},
      pools: [{
        id: 'pool-1',
        targetAgentId: 'claude',
        surface: 'messages',
        dialect: 'claude',
        unifiedGatewayEnrolled: true,
        members: [{
          sourceKind: 'account',
          sourceId: 'acc-1',
          enabled: true,
        }],
      }],
      filter: 'all',
      profileState: 'ready',
      loading: false,
    });
    expect(snapshot.kind).toBe('noRoutes');
    expect(snapshot.hasEnrolledLogins).toBe(true);
  });

  it('shows runningEmpty when listener is up but no traces yet', () => {
    const snapshot = resolveActivityPageSnapshot({
      profiles: [profile],
      bridgeStatuses: {
        'route-a': { profileId: 'route-a', state: 'running' },
      },
      filter: 'all',
      profileState: 'ready',
      loading: false,
    });
    expect(snapshot.kind).toBe('runningEmpty');
  });

  it('merges traces from local gateway status payload', () => {
    const trace = {
      requestId: 'req-1',
      at: '2026-01-01T00:00:00.000Z',
      method: 'POST',
      path: '/v1/messages',
      httpStatus: 200,
      ok: true,
      localAuth: { status: 'ok' as const },
      pool: { status: 'ok' as const },
      conversion: { status: 'ok' as const, path: 'passthrough' },
      upstreamAuth: { status: 'ok' as const },
      upstream: { status: 'ok' as const },
    };
    const snapshot = resolveActivityPageSnapshot({
      profiles: [profile],
      bridgeStatuses: {},
      localGatewayStatuses: [{
        profileId: 'route-a',
        state: 'running',
        recentRouteTraces: [trace],
      }],
      filter: 'all',
      profileState: 'ready',
      loading: false,
    });
    expect(snapshot.kind).toBe('ready');
    expect(snapshot.feed).toHaveLength(1);
  });

  it('merges unauthenticated traces from local gateway status', () => {
    const trace = {
      requestId: 'req-unauth',
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
      failureStage: 'local_auth',
    };
    const snapshot = resolveActivityPageSnapshot({
      profiles: [profile],
      bridgeStatuses: {
        'route-a': { profileId: 'route-a', state: 'running' },
      },
      unauthenticatedTraces: [trace],
      unauthenticatedSourceLabel: '未绑定路由',
      filter: 'all',
      profileState: 'ready',
      loading: false,
    });
    expect(snapshot.kind).toBe('ready');
    expect(snapshot.feed.some((row) => row.unauthenticated)).toBe(true);
  });
});
