import { describe, expect, it } from 'vitest';
import type { AdapterProfile, DefaultRoutePoolOverview } from '@/lib/backend/contracts/adapter';
import type { GatewayUsageRow } from '@/lib/backend/contracts/usage-types';
import {
  attachTokenUsage,
  buildLocalTokenRows,
  generateLocalToken,
  lastVisitFromStatuses,
  maskLocalToken,
  tokenTypeLabel,
  visibleTokenKinds,
} from './tokens-model';

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

function usageRow(
  partial: Partial<GatewayUsageRow> & Pick<GatewayUsageRow, 'requestId' | 'profileId'>,
): GatewayUsageRow {
  return {
    ts: '2026-08-31T09:00:00.000Z',
    surface: 'responses',
    inputTokens: 0,
    outputTokens: 0,
    status: 'ok',
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

  it('generates ahb_ keys with unpadded base64url payload', () => {
    const first = generateLocalToken(() => new Uint8Array(32).fill(1));
    const second = generateLocalToken(() => new Uint8Array(32).fill(2));
    expect(first).toMatch(/^ahb_[A-Za-z0-9_-]+$/);
    expect(first).not.toContain('+');
    expect(first).not.toContain('/');
    expect(first).not.toContain('=');
    expect(first).not.toBe(second);
    expect(generateLocalToken()).toMatch(/^ahb_/);
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

  it('omits hidden Agents from board entry-key kinds', () => {
    expect(visibleTokenKinds(
      [
        { kind: 'messages', targetAgentId: 'claude' },
        { kind: 'chat_completions', targetAgentId: 'cursor' },
      ],
      new Set(['cursor']),
    )).toEqual(['messages']);
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

  it('records last visited page from the newest inbound request', () => {
    expect(lastVisitFromStatuses(
      ['a', 'b'],
      {
        a: {
          profileId: 'a',
          state: 'running',
          lastRequestAt: '2026-08-31T10:00:00.000Z',
          recentInbound: [{
            at: '2026-08-31T10:00:00.000Z',
            method: 'POST',
            path: '/v1/messages',
            status: 200,
            ok: true,
          }],
        },
        b: {
          profileId: 'b',
          state: 'running',
          lastRequestAt: '2026-08-31T12:00:00.000Z',
          recentInbound: [{
            at: '2026-08-31T12:00:00.000Z',
            method: 'GET',
            path: '/v1/models',
            status: 200,
            ok: true,
          }],
        },
      },
    )).toEqual({
      lastPath: '/v1/models',
      lastRequestAt: '2026-08-31T12:00:00.000Z',
    });
  });

  it('keeps last visit on pool rows from runtime status', () => {
    const rows = buildLocalTokenRows(
      [profile({
        id: 'codex-bridge',
        name: 'Codex',
        targetAgentId: 'codex',
        sourceId: 'src-codex',
        localPort: 8101,
      })],
      {
        'codex-bridge': {
          profileId: 'codex-bridge',
          state: 'running',
          port: 8101,
          upstreamStatus: 'connected',
          localToken: 'ahb_secret',
          lastRequestAt: '2026-08-31T09:00:00.000Z',
          recentInbound: [{
            at: '2026-08-31T09:00:00.000Z',
            method: 'POST',
            path: '/v1/responses',
            status: 200,
            ok: true,
          }],
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
      ],
    );
    expect(rows[0]).toMatchObject({
      profileIds: ['pool-codex', 'codex-bridge'],
      lastPath: '/v1/responses',
      lastRequestAt: '2026-08-31T09:00:00.000Z',
    });
  });

  it('sums token usage for the matching profiles and surface', () => {
    const rows = attachTokenUsage(
      buildLocalTokenRows(
        [profile({
          id: 'codex-bridge',
          name: 'Codex',
          targetAgentId: 'codex',
          sourceId: 'src-codex',
        })],
        {},
        {},
        [
          pool({
            id: 'pool-codex',
            targetAgentId: 'codex',
            dialect: 'codex',
            members: [{ sourceKind: 'provider', sourceId: 'src-codex', enabled: true }],
          }),
        ],
      ),
      [
        usageRow({
          requestId: 'keep',
          profileId: 'codex-bridge',
          surface: 'responses',
          inputTokens: 100,
          outputTokens: 40,
        }),
        usageRow({
          requestId: 'other-surface',
          profileId: 'codex-bridge',
          surface: 'chat',
          inputTokens: 999,
          outputTokens: 999,
        }),
        usageRow({
          requestId: 'other-profile',
          profileId: 'other',
          surface: 'responses',
          inputTokens: 50,
          outputTokens: 50,
        }),
      ],
    );
    expect(rows[0].usage).toEqual({
      requestCount: 1,
      inputTokens: 100,
      outputTokens: 40,
      cachedInputTokens: 0,
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

  it('copies listed models onto the token row and merges shared chat models', () => {
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
          listedModels: ['gpt-4o', 'kimi-k2'],
        }),
        pool({
          id: 'pool-kimi',
          targetAgentId: 'kimi',
          surface: 'chat_completions',
          dialect: 'kimi',
          members: [{ sourceKind: 'provider', sourceId: 'kimi-1', enabled: true }],
          listedModels: ['kimi-k2', 'kimi-k2.5'],
        }),
      ],
      true,
    );
    expect(rows[0].listedModels).toEqual(['kimi-k2', 'kimi-k2.5', 'gpt-4o']);
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

  it('prefers the running listener bearer over the stored pool hub token', () => {
    const rows = buildLocalTokenRows(
      [
        profile({
          id: 'adapter-codex-kimi-bridge',
          targetAgentId: 'kimi',
          localPort: 44227,
        }),
      ],
      {
        'adapter-codex-kimi-bridge': {
          profileId: 'adapter-codex-kimi-bridge',
          state: 'running',
          port: 44227,
          localToken: 'ahb_listener_ok_Y5RM',
          upstreamStatus: 'connected',
        },
      },
      {},
      [
        pool({
          id: 'adapter-codex-kimi-bridge',
          targetAgentId: 'kimi',
          surface: 'chat_completions',
          dialect: 'kimi',
          gatewayPort: 44227,
          members: [{ sourceKind: 'account', sourceId: 'codex-1', enabled: true }],
        }),
      ],
      false,
      { 'adapter-codex-kimi-bridge': 'ahb_hub_token_2zpU' },
    );
    expect(rows).toHaveLength(1);
    expect(rows[0]?.token).toBe('ahb_listener_ok_Y5RM');
    expect(rows[0]?.maskedToken).toBe('ahb_••••Y5RM');
  });

});
