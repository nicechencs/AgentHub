import { beforeEach, describe, expect, it } from 'vitest';
import { createMockAccountPort, resetMockAccounts } from './account';

describe('mock OAuth sessions', () => {
  beforeEach(() => {
    resetMockAccounts();
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
