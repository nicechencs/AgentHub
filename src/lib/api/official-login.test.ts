import { beforeEach, describe, expect, it, vi } from 'vitest';

const startOAuth = vi.fn();
const waitOAuth = vi.fn();
const finishOAuth = vi.fn();
const cancelOAuth = vi.fn();
const startDeviceOAuth = vi.fn();
const pollDeviceOAuth = vi.fn();
const finishDeviceOAuth = vi.fn();

vi.mock('@/lib/api/account', () => ({
  startOAuth: (...args: unknown[]) => startOAuth(...args),
  waitOAuth: (...args: unknown[]) => waitOAuth(...args),
  finishOAuth: (...args: unknown[]) => finishOAuth(...args),
  cancelOAuth: (...args: unknown[]) => cancelOAuth(...args),
  startDeviceOAuth: (...args: unknown[]) => startDeviceOAuth(...args),
  pollDeviceOAuth: (...args: unknown[]) => pollDeviceOAuth(...args),
  finishDeviceOAuth: (...args: unknown[]) => finishDeviceOAuth(...args),
}));

import {
  cancelOfficialLogin,
  finishOfficialLogin,
  pollOfficialLogin,
  startOfficialLogin,
} from './official-login';

describe('official login session façade', () => {
  beforeEach(() => {
    startOAuth.mockReset();
    waitOAuth.mockReset();
    finishOAuth.mockReset();
    cancelOAuth.mockReset();
    startDeviceOAuth.mockReset();
    pollDeviceOAuth.mockReset();
    finishDeviceOAuth.mockReset();
  });

  it('starts / polls / finishes a browser login through the PKCE adapter', async () => {
    startOAuth.mockResolvedValue({
      state: 'pkce-1',
      authorizeUrl: 'https://example.test/auth',
      redirectUri: 'http://127.0.0.1:1455/callback',
      agentId: 'claude',
      providerKey: null,
      browserOpened: true,
    });
    waitOAuth.mockResolvedValue({
      state: 'pkce-1',
      agentId: 'claude',
      status: 'callbackReceived',
      error: null,
    });
    finishOAuth.mockResolvedValue({ id: 'acc-1', agentId: 'claude', kind: 'oauth' });

    const session = await startOfficialLogin('claude', { id: 'claude', flow: 'pkce' });
    expect(startOAuth).toHaveBeenCalledWith('claude', true, 'claude');
    expect(startDeviceOAuth).not.toHaveBeenCalled();
    expect(session.flow).toBe('pkce');
    expect(session.sessionId).toBe('pkce-1');

    const poll = await pollOfficialLogin(session);
    expect(waitOAuth).toHaveBeenCalledWith('pkce-1', 120);
    expect(poll.phase).toBe('ready');

    await finishOfficialLogin(session);
    expect(finishOAuth).toHaveBeenCalledWith('pkce-1');
    expect(finishDeviceOAuth).not.toHaveBeenCalled();
  });

  it('starts / polls / finishes a device-code login through the device adapter', async () => {
    startDeviceOAuth.mockResolvedValue({
      state: 'dev-1',
      agentId: 'pi',
      providerKey: 'xai',
      userCode: 'ABCD-EFGH',
      verificationUri: 'https://auth.x.ai/device',
      intervalSecs: 5,
      expiresInSecs: 900,
    });
    pollDeviceOAuth.mockResolvedValue({ state: 'dev-1', status: 'complete', error: null });
    finishDeviceOAuth.mockResolvedValue({ id: 'acc-2', agentId: 'pi', kind: 'oauth' });

    const session = await startOfficialLogin('pi', { id: 'xai', flow: 'deviceCode' });
    expect(startDeviceOAuth).toHaveBeenCalledWith('pi', 'xai');
    expect(startOAuth).not.toHaveBeenCalled();
    expect(session.flow).toBe('deviceCode');
    expect(session.userCode).toBe('ABCD-EFGH');

    const poll = await pollOfficialLogin(session);
    expect(pollDeviceOAuth).toHaveBeenCalledWith('dev-1');
    expect(poll.phase).toBe('ready');

    await finishOfficialLogin(session);
    expect(finishDeviceOAuth).toHaveBeenCalledWith('dev-1');
  });

  it('cancels by session id for either adapter', async () => {
    await cancelOfficialLogin({
      sessionId: 'pkce-1',
      agentId: 'claude',
      optionId: 'claude',
      flow: 'pkce',
      intervalSecs: 0,
      expiresInSecs: 120,
    });
    expect(cancelOAuth).toHaveBeenCalledWith('pkce-1');
    await cancelOfficialLogin('dev-1');
    expect(cancelOAuth).toHaveBeenCalledWith('dev-1');
    await cancelOfficialLogin(null);
    expect(cancelOAuth).toHaveBeenCalledTimes(2);
  });
});
