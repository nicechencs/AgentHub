import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  Sub2ApiError,
  buildLoginBody,
  fetchPublicSettings,
  isTotp2FARequired,
  listApiKeys,
  loginWith2FA,
  loginWithPassword,
  resolveCaptchaKind,
} from './client';
import { maskApiKey } from './url';

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

function calledUrl(fetchImpl: ReturnType<typeof vi.fn>): string {
  const calls = fetchImpl.mock.calls as unknown as Array<[unknown, ...unknown[]]>;
  return String(calls[0]?.[0] ?? '');
}

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

  it('posts native login without logging password and detects 2FA', async () => {
    const fetchImpl = vi.fn(async (_url: string, init?: RequestInit) => {
      const body = JSON.parse(String(init?.body ?? '{}')) as Record<string, unknown>;
      expect(body.email).toBe('a@b.c');
      expect(body.password).toBe('secret');
      expect(body.turnstile_token).toBe('tok');
      return new Response(
        JSON.stringify({
          code: 0,
          message: 'ok',
          data: { requires_2fa: true, temp_token: 'tmp', user_email_masked: 'a***@b.c' },
        }),
        { status: 200, headers: { 'Content-Type': 'application/json' } },
      );
    });
    vi.stubGlobal('fetch', fetchImpl);
    const result = await loginWithPassword(
      'https://v2.pincc.ai',
      buildLoginBody('a@b.c', 'secret', { turnstile_token: 'tok' }),
    );
    expect(isTotp2FARequired(result)).toBe(true);
    if (isTotp2FARequired(result)) {
      expect(result.temp_token).toBe('tmp');
    }
    expect(fetchImpl).toHaveBeenCalledTimes(1);
    expect(calledUrl(fetchImpl)).toContain('/auth/login');
  });

  it('posts 2FA completion', async () => {
    const fetchImpl = vi.fn(async () =>
      new Response(
        JSON.stringify({
          code: 0,
          message: 'ok',
          data: { access_token: 'at', refresh_token: 'rt', expires_in: 3600, user: { id: 1 } },
        }),
        { status: 200, headers: { 'Content-Type': 'application/json' } },
      ),
    );
    vi.stubGlobal('fetch', fetchImpl);
    const tokens = await loginWith2FA('https://v2.pincc.ai', {
      temp_token: 'tmp',
      totp_code: '123456',
    });
    expect(tokens.access_token).toBe('at');
    expect(calledUrl(fetchImpl)).toContain('/auth/login/2fa');
  });

  it('resolves captcha kind from public settings', () => {
    expect(resolveCaptchaKind({})).toBe('none');
    expect(
      resolveCaptchaKind({ turnstile_enabled: true, turnstile_site_key: 'pk' }),
    ).toBe('turnstile');
    expect(
      resolveCaptchaKind({
        tencent_captcha_enabled: true,
        tencent_captcha_app_id: 'app',
      }),
    ).toBe('tencent');
    expect(
      resolveCaptchaKind({
        aliyun_captcha_enabled: true,
        aliyun_captcha_scene_id: 'sc',
        aliyun_captcha_prefix: 'pfx',
      }),
    ).toBe('aliyun');
    expect(
      resolveCaptchaKind({
        turnstile_enabled: true,
        turnstile_site_key: 'pk',
        tencent_captcha_enabled: true,
        tencent_captcha_app_id: 'app',
      }),
    ).toBe('turnstile');
  });

  it('omits empty captcha fields from login body', () => {
    expect(buildLoginBody(' a@b.c ', 'pw', { turnstile_token: '  ' })).toEqual({
      email: 'a@b.c',
      password: 'pw',
    });
    expect(
      buildLoginBody('a@b.c', 'pw', {
        tencent_captcha_ticket: 't',
        tencent_captcha_randstr: 'r',
      }),
    ).toEqual({
      email: 'a@b.c',
      password: 'pw',
      tencent_captcha_ticket: 't',
      tencent_captcha_randstr: 'r',
    });
  });
});
