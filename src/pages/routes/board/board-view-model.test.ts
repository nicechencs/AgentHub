import { describe, expect, it } from 'vitest';
import type { AdapterProfile } from '@/lib/backend/contracts/adapter';
import {
  buildRouteBoardStatusRows,
  mergeRecentInbound,
} from './board-view-model';

function profile(partial: Partial<AdapterProfile> & Pick<AdapterProfile, 'id'>): AdapterProfile {
  return {
    name: partial.name ?? partial.id,
    sourceKind: 'provider',
    sourceId: 'src',
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
  it('orders status rows with errors first', () => {
    const rows = buildRouteBoardStatusRows(
      [
        profile({ id: 'ok', name: 'Ok' }),
        profile({ id: 'bad', name: 'Bad' }),
      ],
      {
        ok: { profileId: 'ok', state: 'running', port: 1, upstreamStatus: 'connected' },
        bad: { profileId: 'bad', state: 'error', port: 2, upstreamStatus: 'unavailable' },
      },
    );
    expect(rows.map((row) => row.profileId)).toEqual(['bad', 'ok']);
    expect(rows[0].endpoint).toBe('127.0.0.1:2');
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
});
