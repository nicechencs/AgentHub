import { describe, expect, it } from 'vitest';
import type { AdapterProfile } from '@/lib/backend/contracts/adapter';
import { buildLocalTokenRows } from './tokens-model';

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

describe('tokens-model', () => {
  it('lists only local_bridge routes with token and endpoint', () => {
    const rows = buildLocalTokenRows(
      [
        profile({ id: 'bridge', name: 'Claude', localPort: 8101 }),
        profile({ id: 'direct', name: 'Direct', route: 'native_endpoint' }),
      ],
      {
        bridge: {
          profileId: 'bridge',
          state: 'running',
          port: 8101,
          upstreamStatus: 'connected',
          localToken: 'ahb_secret',
        },
      },
    );
    expect(rows).toHaveLength(1);
    expect(rows[0]).toMatchObject({
      profileId: 'bridge',
      name: 'Claude',
      endpoint: '127.0.0.1:8101',
      state: 'running',
      token: 'ahb_secret',
    });
  });

  it('falls back to target agent name and profile port when status is missing', () => {
    const rows = buildLocalTokenRows(
      [profile({ id: 'p1', name: '   ', targetAgentId: 'codex', localPort: 0 })],
      {},
    );
    expect(rows[0].name).toBe('codex');
    expect(rows[0].endpoint).toBeNull();
    expect(rows[0].token).toBeNull();
  });

  it('sorts rows by name', () => {
    const rows = buildLocalTokenRows(
      [profile({ id: 'b', name: 'Bravo' }), profile({ id: 'a', name: 'Alpha' })],
      {},
    );
    expect(rows.map((row) => row.name)).toEqual(['Alpha', 'Bravo']);
  });
});
