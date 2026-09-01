import { describe, expect, it } from 'vitest';
import { buildTokenDetailCopyRows, tokenEndpointParts } from './token-detail-model';
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
});
