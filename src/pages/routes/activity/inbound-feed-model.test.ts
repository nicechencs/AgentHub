import { describe, expect, it } from 'vitest';
import {
  activityRouteOptions,
  buildInboundFeed,
  countFailedInbound,
  filterInboundFeed,
  parseActivityFilter,
} from './inbound-feed-model';
import type { MergedInboundRow } from '../board/board-view-model';

const sample: MergedInboundRow[] = [
  {
    at: '2026-08-12T00:00:02.000Z',
    method: 'POST',
    path: '/v1/ok',
    status: 200,
    ok: true,
    profileId: 'a',
    sourceLabel: 'A',
  },
  {
    at: '2026-08-12T00:00:01.000Z',
    method: 'GET',
    path: '/fail',
    status: 500,
    ok: false,
    profileId: 'a',
    sourceLabel: 'A',
  },
  {
    at: '2026-08-12T00:00:00.000Z',
    method: 'POST',
    path: '/v1/b',
    status: 200,
    ok: true,
    profileId: 'b',
    sourceLabel: 'B',
  },
];

describe('inbound-feed-model', () => {
  it('filters failed rows and optional route', () => {
    expect(filterInboundFeed(sample, 'failed')).toHaveLength(1);
    expect(filterInboundFeed(sample, 'all')).toHaveLength(3);
    expect(filterInboundFeed(sample, 'all', 'b')).toHaveLength(1);
    expect(countFailedInbound(sample)).toBe(1);
  });

  it('builds a capped feed from bridge statuses', () => {
    const feed = buildInboundFeed(
      [{ id: 'a', name: 'A', route: 'local_bridge', targetAgentId: 'claude' }],
      {
        a: {
          profileId: 'a',
          state: 'running',
          recentInbound: sample.filter((row) => row.profileId === 'a'),
        },
      },
      'all',
      10,
    );
    expect(feed[0].path).toBe('/v1/ok');
  });

  it('lists route filter options and parses URL filter', () => {
    expect(parseActivityFilter('failed')).toBe('failed');
    expect(activityRouteOptions([
      { id: 'b', name: 'Beta', route: 'local_bridge', targetAgentId: 'codex' },
      { id: 'a', name: 'Alpha', route: 'local_bridge', targetAgentId: 'claude' },
      { id: 'x', name: 'Skip', route: 'native_endpoint', targetAgentId: 'claude' },
    ]).map((row) => row.id)).toEqual(['a', 'b', 'x']);
  });
});
