import { describe, expect, it } from 'vitest';
import { sub2apiKeyToConnectDraft, sub2apiSyncNavigation } from './sync';

describe('sub2api sync helpers', () => {
  it('builds a Connections add-key draft from gateway + key', () => {
    expect(sub2apiKeyToConnectDraft('https://v2.pincc.ai', { key: 'sk-test', name: 'n' })).toEqual({
      baseUrl: 'https://v2.pincc.ai',
      apiKey: 'sk-test',
    });
  });

  it('navigates to Connections add-key with draft state', () => {
    const nav = sub2apiSyncNavigation('codex', 'https://v2.pincc.ai', { key: 'sk-1', name: 'a' });
    expect(nav.pathname).toContain('/connections?');
    expect(nav.pathname).toContain('intent=add-key');
    expect(nav.state.connectApiKeyDraft.apiKey).toBe('sk-1');
  });
});
