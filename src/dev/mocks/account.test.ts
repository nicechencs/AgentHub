import { beforeEach, describe, expect, it } from 'vitest';
import {
  createMockAccountPort,
  getMockAccountById,
  resetMockAccounts,
} from './account';
import {
  createMockAdapterPort,
  resetMockAdapters,
} from './adapter';
import {
  getMockProviderById,
  removeMockProvider,
  resetMockProviders,
  upsertMockProvider,
} from './provider';

describe('mock OAuth sessions', () => {
  const resolver = {
    getAccountById: getMockAccountById,
    getProviderById: getMockProviderById,
    upsertGeneratedProvider: upsertMockProvider,
    removeGeneratedProvider: removeMockProvider,
  };

  beforeEach(() => {
    resetMockAccounts();
    resetMockAdapters();
    resetMockProviders();
  });

  it('finishOAuth uses the started agent instead of hardcoding claude', async () => {
    const accounts = createMockAccountPort();
    const start = await accounts.startOAuth('grok', true, null);
    const wait = await accounts.waitOAuth(start.state);
    expect(wait.agentId).toBe('grok');
    expect(wait.status).toBe('callbackReceived');

    const acc = await accounts.finishOAuth(start.state);
    expect(acc.agentId).toBe('grok');
    expect(acc.kind).toBe('oauth');
    expect(acc.subscription).toBe('SuperGrok');
    expect((await accounts.listAccounts('grok')).some((row) => row.account.id === acc.id)).toBe(true);
    expect((await accounts.listAccounts('claude')).some((row) => row.account.id === acc.id)).toBe(false);
  });

  it('finishOAuth forwards the session providerKey', async () => {
    const accounts = createMockAccountPort();
    const start = await accounts.startOAuth('pi', true, 'anthropic');
    const acc = await accounts.finishOAuth(start.state);
    expect(acc.agentId).toBe('pi');
    expect(acc.label).toMatch(/^pi:anthropic · /);
  });

  it('lists Kiro official login as CLI-guided, not browser PKCE', async () => {
    const accounts = createMockAccountPort();
    expect(await accounts.oauthSupported('kiro')).toBe(true);
    const opts = await accounts.listOAuthOptions('kiro');
    expect(opts).toHaveLength(1);
    expect(opts[0]).toMatchObject({
      id: 'kiro',
      agentId: 'kiro',
      flow: 'cli',
    });
  });

  it('lists Grok official login as device-code, not browser PKCE', async () => {
    const accounts = createMockAccountPort();
    const opts = await accounts.listOAuthOptions('grok');
    expect(opts).toHaveLength(1);
    expect(opts[0]).toMatchObject({
      id: 'xai',
      agentId: 'grok',
      flow: 'deviceCode',
    });
  });

  it('finishDeviceOAuth uses the started device session instead of hardcoding pi/xai', async () => {
    const accounts = createMockAccountPort();
    const start = await accounts.startDeviceOAuth('grok', 'xai');
    const first = await accounts.pollDeviceOAuth(start.state);
    expect(first.status).toBe('pending');
    const poll = await accounts.pollDeviceOAuth(start.state);
    expect(poll.status).toBe('complete');

    const acc = await accounts.finishDeviceOAuth(start.state);
    expect(acc.agentId).toBe('grok');
    expect(acc.kind).toBe('oauth');
    expect((await accounts.listAccounts('pi')).some((row) => row.account.id === acc.id)).toBe(false);
  });

  it('pool-owned finishDeviceOAuth enrolls with create-only RoundRobin schedulePolicy', async () => {
    const accounts = createMockAccountPort();
    const adapter = createMockAdapterPort(resolver);

    const start = await accounts.startDeviceOAuth('grok', 'xai', true);
    await accounts.pollDeviceOAuth(start.state);
    await accounts.pollDeviceOAuth(start.state);
    const acc = await accounts.finishDeviceOAuth(start.state, true, 'round_robin');

    expect(acc.home).toBe('route_pool');
    expect(getMockAccountById(acc.id)?.home).toBe('route_pool');

    const listed = await adapter.listDefaultRoutePools();
    const grokPool = listed.pools.find((pool) => (
      pool.targetAgentId === 'grok' && pool.surface === 'responses'
    ));
    expect(grokPool).toBeTruthy();
    expect(grokPool?.schedulePolicy).toBe('round_robin');
    expect(grokPool?.members).toEqual([
      expect.objectContaining({ sourceKind: 'account', sourceId: acc.id, enabled: true }),
    ]);

    // Create-only: a second pool-owned finish must not overwrite an existing policy.
    const start2 = await accounts.startDeviceOAuth('grok', 'xai', true);
    await accounts.pollDeviceOAuth(start2.state);
    await accounts.pollDeviceOAuth(start2.state);
    const acc2 = await accounts.finishDeviceOAuth(start2.state, true, 'priority_failover');
    const listed2 = await adapter.listDefaultRoutePools();
    const grokPool2 = listed2.pools.find((pool) => (
      pool.targetAgentId === 'grok' && pool.surface === 'responses'
    ));
    expect(grokPool2?.schedulePolicy).toBe('round_robin');
    expect(grokPool2?.members.some((m) => m.sourceId === acc2.id)).toBe(true);
  });

  it('rejects unknown state instead of finishing as claude or pi', async () => {
    const accounts = createMockAccountPort();
    await expect(accounts.waitOAuth('missing-state')).rejects.toThrow(/unknown oauth state/i);
    await expect(accounts.finishOAuth('missing-state')).rejects.toThrow(/unknown oauth state/i);
    await expect(accounts.pollDeviceOAuth('missing-state')).rejects.toThrow(/unknown oauth state/i);
    await expect(accounts.finishDeviceOAuth('missing-state')).rejects.toThrow(/unknown oauth state/i);
  });

  it('cancelOAuth removes the session so later wait/finish fail', async () => {
    const accounts = createMockAccountPort();
    const start = await accounts.startOAuth('codex', true, null);
    await accounts.cancelOAuth(start.state);
    await expect(accounts.waitOAuth(start.state)).rejects.toThrow(/unknown oauth state/i);
    await expect(accounts.finishOAuth(start.state)).rejects.toThrow(/unknown oauth state/i);
  });

  it('resetMockAccounts clears in-flight OAuth sessions', async () => {
    const accounts = createMockAccountPort();
    const start = await accounts.startOAuth('claude', true, null);
    resetMockAccounts();
    await expect(accounts.waitOAuth(start.state)).rejects.toThrow(/unknown oauth state/i);
    await expect(accounts.finishOAuth(start.state)).rejects.toThrow(/unknown oauth state/i);
  });
});
