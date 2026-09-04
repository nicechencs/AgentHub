import { describe, expect, it } from 'vitest';
import type { AdapterProfile, DefaultRoutePoolOverview } from '@/lib/backend/contracts/adapter';
import {
  agentSupportsLocalEndpointKind,
  buildCreateTokenEndpointCards,
  buildLocalTokenGroups,
  buildLocalTokenRows,
  defaultCreateTokenName,
  firstCreateTokenPoolId,
  generateLocalToken,
  localTokenDeleteGate,
  localTokenEditKeyGate,
  localTokenEmptyCreateGate,
  maskLocalToken,
  supportedAgentsForEndpointKind,
  tokenDisplayName,
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

function pool(
  partial: Partial<DefaultRoutePoolOverview> & Pick<DefaultRoutePoolOverview, 'id'>,
): DefaultRoutePoolOverview {
  return {
    targetAgentId: 'codex',
    surface: 'responses',
    dialect: 'codex',
    unifiedGatewayEnrolled: true,
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
      id: 'pool-codex',
      poolBacked: true,
      profileId: 'codex-bridge',
      endpoint: '127.0.0.1:8101',
      token: 'ahb_secret',
    });
    expect(localTokenEditKeyGate(rows[0]).enabled).toBe(true);
    expect(localTokenEditKeyGate(rows[0]).reason).toBeNull();
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
    expect(rows[0]).toMatchObject({
      id: 'p1',
      poolBacked: false,
      profileId: 'p1',
    });
    expect(localTokenEditKeyGate(rows[0]).enabled).toBe(false);
    expect(localTokenEditKeyGate(rows[0]).reason).toContain('连接池入口 Key');
    expect(localTokenDeleteGate(rows[0]).enabled).toBe(false);
  });

  it('lists named extra keys under the same type and blocks deleting the last key', () => {
    const rows = buildLocalTokenRows(
      [
        profile({
          id: 'codex-bridge',
          name: 'Codex',
          targetAgentId: 'codex',
          sourceId: 'src-codex',
          localPort: 8101,
        }),
      ],
      {},
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
      false,
      {},
      [
        {
          id: 'pool-codex',
          poolId: 'pool-codex',
          token: 'ahb_default',
          name: '默认',
          primary: true,
        },
        {
          id: 'extra-home',
          poolId: 'pool-codex',
          token: 'ahb_home_key',
          name: '家里',
          primary: false,
        },
      ],
    );
    expect(rows).toHaveLength(2);
    expect(tokenDisplayName(rows[0])).toBe('默认');
    expect(rows[0]).toMatchObject({ id: 'pool-codex', canDelete: true, primary: true });
    expect(localTokenDeleteGate(rows[0], rows).enabled).toBe(true);
    expect(rows[1]).toMatchObject({
      id: 'extra-home',
      name: '家里',
      token: 'ahb_home_key',
      canDelete: true,
      primary: false,
    });
    expect(localTokenDeleteGate(rows[1], rows).enabled).toBe(true);
    expect(localTokenDeleteGate(rows[0], [rows[0]]).enabled).toBe(false);
    expect(localTokenDeleteGate(rows[0], [rows[0]]).reason).toContain('只剩这一把');
  });

  it('names an empty default key 默认 instead of the type label', () => {
    expect(tokenDisplayName({ name: '', kind: 'messages', primary: true })).toBe('默认');
    expect(tokenDisplayName({ name: '工作电脑', kind: 'messages', primary: true })).toBe('工作电脑');
    expect(tokenDisplayName({ name: '', kind: 'messages', primary: false })).toBe('Messages');
  });

  it('groups rows by endpoint type with the shared address on the group', () => {
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
          id: 'claude-bridge',
          name: 'Claude',
          targetAgentId: 'claude',
          sourceId: 'src-claude',
          localPort: 8100,
        }),
      ],
      {
        'codex-bridge': {
          profileId: 'codex-bridge',
          state: 'running',
          port: 8101,
          upstreamStatus: 'connected',
          localToken: 'ahb_secret',
        },
        'claude-bridge': {
          profileId: 'claude-bridge',
          state: 'running',
          port: 8100,
          upstreamStatus: 'connected',
          localToken: 'ahb_claude',
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
          id: 'pool-claude',
          targetAgentId: 'claude',
          surface: 'messages',
          dialect: 'claude',
          members: [{ sourceKind: 'provider', sourceId: 'src-claude', enabled: true }],
          gatewayPort: 8100,
        }),
      ],
    );
    expect(buildLocalTokenGroups(rows).map((group) => group.kind)).toEqual([
      'messages',
      'responses_codex',
    ]);
    expect(buildLocalTokenGroups(rows)[0]).toMatchObject({
      path: '/v1/messages',
      endpoint: '127.0.0.1:8100',
    });
  });

  it('keeps an empty pool row when listLocalTokens did not return a key', () => {
    const rows = buildLocalTokenRows(
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
      false,
      {},
      [],
    );
    expect(rows).toHaveLength(1);
    expect(rows[0]).toMatchObject({
      id: 'pool-codex',
      poolBacked: true,
      token: null,
      maskedToken: null,
    });
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

  it('shows the pool entry key when the local gateway is stopped', () => {
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

  it('does not expose the stored hub token while the listener is up without localToken', () => {
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
    expect(rows[0]?.token).toBeNull();
    expect(rows[0]?.maskedToken).toBeNull();
  });


  it('marks pool rows editable and leftover profile rows not editable for setLocalToken', () => {
    const poolRows = buildLocalTokenRows(
      [
        profile({
          id: 'codex-bridge',
          name: 'Codex',
          targetAgentId: 'codex',
          sourceId: 'src-codex',
          localPort: 8101,
        }),
      ],
      {},
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
    const leftoverRows = buildLocalTokenRows(
      [profile({ id: 'orphan-bridge', name: 'Orphan', targetAgentId: 'claude' })],
      {
        'orphan-bridge': {
          profileId: 'orphan-bridge',
          state: 'running',
          port: 9001,
          upstreamStatus: 'connected',
          localToken: 'ahb_orphan',
        },
      },
    );
    expect(poolRows[0]).toMatchObject({ id: 'pool-codex', poolBacked: true });
    expect(localTokenEditKeyGate(poolRows[0])).toEqual({ enabled: true, reason: null });
    expect(leftoverRows[0]).toMatchObject({
      id: 'orphan-bridge',
      poolBacked: false,
      token: 'ahb_orphan',
      profileId: 'orphan-bridge',
    });
    const leftoverGate = localTokenEditKeyGate(leftoverRows[0]);
    expect(leftoverGate.enabled).toBe(false);
    expect(leftoverGate.reason).toBe('这条还不是连接池入口 Key，先从路由建入口');
    // Import still needs profileId+token; leftovers keep both — do not block #231 pool import path.
    expect(leftoverRows[0].profileId).toBeTruthy();
    expect(leftoverRows[0].token).toBeTruthy();
  });

  it('lists Agents that speak each endpoint, with Grok only on Grok Responses',
    () => {
      expect(supportedAgentsForEndpointKind('messages')).toEqual([
        'claude',
        'kimi',
        'pi',
        'dsh',
        'zcode',
      ]);
      expect(supportedAgentsForEndpointKind('responses_codex')).toEqual([
        'codex',
        'kimi',
        'pi',
        'dsh',
        'zcode',
      ]);
      expect(supportedAgentsForEndpointKind('responses_grok')).toEqual(['grok']);
      expect(supportedAgentsForEndpointKind('chat_completions')).toEqual([
        'kimi',
        'pi',
        'workbuddy',
        'dsh',
        'zcode',
      ]);
      expect(agentSupportsLocalEndpointKind('grok', 'responses_codex')).toBe(false);
      expect(agentSupportsLocalEndpointKind('codex', 'responses_grok')).toBe(false);
      expect(agentSupportsLocalEndpointKind('cursor', 'messages')).toBe(false);
    });

  it('tiles four endpoint cards and only enables kinds with a pool',
    () => {
      const cards = buildCreateTokenEndpointCards([
        { id: 'pool-claude', kind: 'messages' },
        { id: 'pool-kimi', kind: 'chat_completions' },
      ]);
      expect(cards.map((card) => card.kind)).toEqual([
        'messages',
        'responses_codex',
        'responses_grok',
        'chat_completions',
      ]);
      expect(cards[0]).toMatchObject({
        path: '/v1/messages',
        poolId: 'pool-claude',
      });
      expect(cards[1]?.poolId).toBeNull();
      expect(cards[2]?.poolId).toBeNull();
      expect(cards[3]).toMatchObject({
        path: '/v1/chat/completions',
        poolId: 'pool-kimi',
      });
      expect(firstCreateTokenPoolId(cards)).toBe('pool-claude');
      expect(firstCreateTokenPoolId([])).toBe('');
    });

  it('enables 为此端点新建 on pool-backed empty rows and uses pool hints otherwise', () => {
    expect(localTokenEmptyCreateGate({ poolBacked: true, kind: 'messages' }).enabled).toBe(true);
    expect(localTokenEmptyCreateGate(
      { poolBacked: false, kind: 'chat_completions' },
      { chat_completions: 'pool-kimi' },
    )).toEqual({ enabled: true, reason: null });
    expect(localTokenEmptyCreateGate({ poolBacked: false, kind: 'messages' }).reason)
      .toContain('连接池');
    expect(localTokenEmptyCreateGate(
      { poolBacked: false, kind: 'messages' },
      {},
      undefined,
      true,
    ).reason).toContain('本机转发');
  });

  it('defaults an empty create name to the endpoint label plus a free number', () => {
    expect(defaultCreateTokenName({ kind: 'messages' })).toBe('Messages 2');
    expect(defaultCreateTokenName({
      kind: 'chat_completions',
      existingNames: ['Chat Completions 2', '家里'],
    })).toBe('Chat Completions 3');
  });
});
