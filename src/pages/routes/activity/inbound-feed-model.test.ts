import { describe, expect, it } from 'vitest';
import {
  buildInboundFeed,
  countFailedInbound,
  filterInboundFeed,
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
];

describe('inbound-feed-model', () => {
  it('filters failed rows', () => {
    expect(filterInboundFeed(sample, 'failed')).toHaveLength(1);
    expect(filterInboundFeed(sample, 'all')).toHaveLength(2);
    expect(countFailedInbound(sample)).toBe(1);
  });

  it('builds a capped feed from bridge statuses', () => {
    const feed = buildInboundFeed(
      [{ id: 'a', name: 'A', route: 'local_bridge', targetAgentId: 'claude' }],
      {
        a: {
          profileId: 'a',
          state: 'running',
          recentInbound: sample,
        },
      },
      'all',
      10,
    );
    expect(feed[0].path).toBe('/v1/ok');
  });
});
