import { describe, expect, it, vi } from 'vitest';
import { Sub2ApiError, fetchPublicSettings, listApiKeys } from './client';
import { maskApiKey } from './url';

describe('sub2api client', () => {
  it('masks secrets for display', () => {
    expect(maskApiKey('sk-abcdefghijklmnop')).toBe('sk-a…mnop');
  });

  it('unwraps envelope data on success', async () => {
    const fetchImpl = vi.fn(async () =>
      new Response(JSON.stringify({ code: 0, message: 'ok', data: { api_base_url: 'https://x' } }), {
        status: 200,
        headers: { 'Content-Type': 'application/json' },
      }),
    );
    vi.stubGlobal('fetch', fetchImpl);
    await expect(fetchPublicSettings({ siteUrl: 'https://v2.pincc.ai' })).resolves.toEqual({
      api_base_url: 'https://x',
    });
  });

  it('throws Sub2ApiError when code != 0', async () => {
    const fetchImpl = vi.fn(async () =>
      new Response(JSON.stringify({ code: 401, message: 'unauthorized', data: null }), {
        status: 200,
        headers: { 'Content-Type': 'application/json' },
      }),
    );
    vi.stubGlobal('fetch', fetchImpl);
    await expect(
      listApiKeys({ siteUrl: 'https://v2.pincc.ai', accessToken: 'x' }),
    ).rejects.toBeInstanceOf(Sub2ApiError);
  });
});
