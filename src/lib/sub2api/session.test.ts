import { beforeEach, describe, expect, it, vi } from 'vitest';
import { StorageKey } from '@/lib/storage-key';
import {
  clearSub2ApiSession,
  loadSub2ApiSession,
  saveSub2ApiSession,
  sessionFromTokens,
} from './session';

describe('sub2api session store', () => {
  beforeEach(() => {
    const mem = new Map<string, string>();
    vi.stubGlobal('localStorage', {
      getItem: (k: string) => mem.get(k) ?? null,
      setItem: (k: string, v: string) => {
        mem.set(k, v);
      },
      removeItem: (k: string) => {
        mem.delete(k);
      },
    });
  });

  it('round-trips a session without requiring refresh token', () => {
    const session = sessionFromTokens({
      siteUrl: 'https://v2.pincc.ai',
      accessToken: 'tok',
      gatewayBaseUrl: 'https://v2.pincc.ai',
    });
    saveSub2ApiSession(session);
    expect(loadSub2ApiSession()?.accessToken).toBe('tok');
    expect(localStorage.getItem(StorageKey.sub2apiSession)).toContain('v2.pincc.ai');
    clearSub2ApiSession();
    expect(loadSub2ApiSession()).toBeNull();
  });
});
