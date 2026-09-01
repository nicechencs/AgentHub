import { describe, expect, it } from 'vitest';
import {
  buildTokenDetailCopyRows,
  formatTokenRelative,
  tokenEndpointParts,
  tokenLastPageDisplay,
  tokenUsageDisplay,
} from './token-detail-model';
import type { LocalTokenRow } from './tokens-model';

function row(partial: Partial<LocalTokenRow> = {}): LocalTokenRow {
  return {
    id: 'pool-kimi',
    profileId: 'bridge-1',
    name: 'kimi · /v1/chat/completions',
    kind: 'chat_completions',
    path: '/v1/chat/completions',
    endpoint: '127.0.0.1:8123',
    state: 'running',
    token: 'ahb_secret',
    maskedToken: 'ahb_••••cret',
    unavailable: false,
    targetAgentId: 'kimi',
    profileIds: ['bridge-1'],
    lastPath: '/v1/models',
    lastRequestAt: new Date().toISOString(),
    usage: {
      requestCount: 3,
      inputTokens: 1500,
      outputTokens: 200,
      cachedInputTokens: 0,
    },
    ...partial,
  };
}

describe('token-detail-model', () => {
  it('exposes a copyable endpoint URL and a masked token until revealed', () => {
    const masked = buildTokenDetailCopyRows(row(), false);
    expect(tokenEndpointParts(row()).href).toBe('http://127.0.0.1:8123/v1/chat/completions');
    expect(masked.find((item) => item.id === 'type')).toMatchObject({
      display: 'Chat Completions',
      copyValue: null,
    });
    expect(masked.find((item) => item.id === 'endpoint')).toMatchObject({
      copyValue: 'http://127.0.0.1:8123/v1/chat/completions',
      pending: false,
    });
    expect(masked.find((item) => item.id === 'token')).toMatchObject({
      display: 'ahb_••••cret',
      copyValue: 'ahb_secret',
    });
    const revealed = buildTokenDetailCopyRows(row(), true);
    expect(revealed.find((item) => item.id === 'token')?.display).toBe('ahb_secret');
  });

  it('leaves the token field empty when no key exists yet', () => {
    const copies = buildTokenDetailCopyRows(row({
      token: null,
      maskedToken: null,
    }), false);
    expect(copies.find((item) => item.id === 'token')).toMatchObject({
      display: '',
      copyValue: null,
      pending: true,
    });
  });

  it('withholds the token when the runtime is unavailable', () => {
    const copies = buildTokenDetailCopyRows(row({
      unavailable: true,
      token: 'ahb_secret',
      maskedToken: 'ahb_••••cret',
    }), true);
    expect(copies.find((item) => item.id === 'token')?.copyValue).toBeNull();
  });

  it('shows last visited page and token usage',
    () => {
      expect(tokenLastPageDisplay(row())).toBe('/v1/models');
      expect(tokenLastPageDisplay(row({ lastPath: null }))).toBe('');
      expect(tokenUsageDisplay(row().usage)).toBe('1.5K in / 200 out');
      expect(tokenUsageDisplay({
        requestCount: 0,
        inputTokens: 0,
        outputTokens: 0,
        cachedInputTokens: 0,
      })).toBe('');
      expect(formatTokenRelative(new Date().toISOString())).toBe('刚刚');
    });
});
