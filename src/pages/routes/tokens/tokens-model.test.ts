import { describe, expect, it } from 'vitest';
import type { AdapterProfile, DefaultRoutePoolOverview } from '@/lib/backend/contracts/adapter';
import { buildLocalTokenRows, maskLocalToken, tokenTypeLabel } from './tokens-model';

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

function pool(
  partial: Partial<DefaultRoutePoolOverview> & Pick<DefaultRoutePoolOverview, 'id'>,
): DefaultRoutePoolOverview {
  return {
    targetAgentId: 'codex',
    surface: 'responses',
    dialect: 'codex',
    v2Enrolled: true,
    members: [{ sourceKind: 'provider', sourceId: 'src-1', enabled: true }],
    listedModels: [],
    ...partial,
  };
}

describe('tokens-model', () => {
  it('masks complete local keys while preserving the prefix and tail', () => {
    expect(maskLocalToken('ahb_0123456789')).toBe('ahb_••••6789');
    expect(maskLocalToken('')).toBe('');
  });

  it('lists pool endpoints with Codex / Grok Responses split', () => {
    const rows = buildLocalTokenRows(
      [
        profile({
          id: 'codex-bridge',
          name: 'Codex',
          targetAgentId: 'codex',
          sourceId: 'src-codex',
          localPort: 8101,
        }),
        profile({
          id: 'grok-bridge',
          name: 'Grok',
          targetAgentId: 'grok',
          sourceId: 'src-grok',
          localPort: 8102,
        }),
        profile({ id: 'direct', name: 'Direct', route: 'native_endpoint' }),
      ],
      {
        'codex-bridge': {
          profileId: 'codex-bridge',
          state: 'running',
          port: 8101,
          upstreamStatus: 'connected',
          localToken: 'ahb_secret',
        },
        'grok-bridge': {
          profileId: 'grok-bridge',
          state: 'running',
          port: 8102,
          upstreamStatus: 'connected',
          localToken: 'ahb_grok',
        },
      },
      {},
      [
        pool({
          id: 'pool-codex',
          targetAgentId: 'codex',
          dialect: 'codex',
          members: [{ sourceKind: 'provider', sourceId: 'src-codex', enabled: true }],
          gatewayPort: 8101,
        }),
        pool({
          id: 'pool-grok',
          targetAgentId: 'grok',
          dialect: 'grok',
          members: [{ sourceKind: 'provider', sourceId: 'src-grok', enabled: true }],
          gatewayPort: 8102,
        }),
      ],
    );
    expect(rows.map((row) => row.kind)).toEqual(['responses_codex', 'responses_grok']);
    expect(rows.map((row) => row.path)).toEqual(['/v1/responses', '/v1/responses']);
    expect(rows[0]).toMatchObject({
      profileId: 'codex-bridge',
      endpoint: '127.0.0.1:8101',
      token: 'ahb_secret',
    });
  });

  it('falls back to leftover local-bridge profiles when no pool matches', () => {
    const rows = buildLocalTokenRows(
      [profile({ id: 'p1', name: '   ', targetAgentId: 'codex', localPort: 0 })],
      {},
    );
    expect(rows[0].name).toBe('codex');
    expect(rows[0].kind).toBe('responses_codex');
    expect(rows[0].endpoint).toBeNull();
    expect(rows[0].token).toBeNull();
  });

  it('sorts rows by endpoint kind then name', () => {
    const rows = buildLocalTokenRows(
      [
        profile({ id: 'b', name: 'Bravo', targetAgentId: 'codex' }),
        profile({ id: 'a', name: 'Alpha', targetAgentId: 'claude' }),
      ],
      {},
    );
    expect(rows.map((row) => row.kind)).toEqual(['messages', 'responses_codex']);
  });

  it('names rows by token type, not writer Agent', () => {
    expect(tokenTypeLabel({ kind: 'messages' })).toBe('Messages');
    expect(tokenTypeLabel({ kind: 'responses_codex' })).toBe('Responses · Codex');
    expect(tokenTypeLabel({ kind: 'responses_grok' })).toBe('Responses · Grok');
    expect(tokenTypeLabel({ kind: 'chat_completions' })).toBe('Chat Completions');
  });

  it('shows the pool entry key when the local entry is stopped', () => {
    const rows = buildLocalTokenRows(
      [],
      {},
      {},
      [
        pool({
          id: 'pool-kimi',
          targetAgentId: 'kimi',
          surface: 'chat_completions',
          dialect: 'kimi',
          members: [{ sourceKind: 'provider', sourceId: 'kimi-1', enabled: true }],
        }),
      ],
      false,
      { 'pool-kimi': 'ahb_secretkey12' },
    );
    expect(rows).toHaveLength(1);
    expect(rows[0]).toMatchObject({
      token: 'ahb_secretkey12',
      maskedToken: 'ahb_••••ey12',
    });
  });

  it('keeps one chat-completions row when Kimi and DSH share', () => {
    const rows = buildLocalTokenRows(
      [],
      {},
      {},
      [
        pool({
          id: 'pool-dsh',
          targetAgentId: 'dsh',
          surface: 'chat_completions',
          dialect: 'dsh',
          members: [{ sourceKind: 'provider', sourceId: 'dsh-1', enabled: true }],
        }),
        pool({
          id: 'pool-kimi',
          targetAgentId: 'kimi',
          surface: 'chat_completions',
          dialect: 'kimi',
          members: [{ sourceKind: 'provider', sourceId: 'kimi-1', enabled: true }],
        }),
      ],
      true,
    );
    expect(rows).toHaveLength(1);
    expect(rows[0]).toMatchObject({ id: 'pool-kimi', path: '/v1/chat/completions' });
  });

  it('marks failed status reads unavailable and withholds the token', () => {
    const rows = buildLocalTokenRows(
      [profile({ id: 'bridge' })],
      {
        bridge: {
          profileId: 'bridge',
          state: 'running',
          port: 8101,
          upstreamStatus: 'unavailable',
          localToken: 'ahb_secret',
        },
      },
      { bridge: new Error('status unavailable') },
    );
    expect(rows[0]).toMatchObject({ unavailable: true, token: null, maskedToken: null });
  });
});
