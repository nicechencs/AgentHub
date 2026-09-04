import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  Sub2ApiError,
  buildLoginBody,
  fetchPublicSettings,
  isTotp2FARequired,
  listApiKeys,
  loginWith2FA,
  loginWithPassword,
  mapSub2ApiLoginError,
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

  it('surfaces reason on non-0 envelope for friendlier mapping', async () => {
    const fetchImpl = vi.fn(async () =>
      new Response(
        JSON.stringify({
          code: 400,
          message: 'tencent captcha verification failed',
          reason: 'TENCENT_CAPTCHA_VERIFICATION_FAILED',
          data: null,
        }),
        { status: 200, headers: { 'Content-Type': 'application/json' } },
      ),
    );
    vi.stubGlobal('fetch', fetchImpl);
    let caught: unknown;
    try {
      await loginWithPassword(
        'https://v2.pincc.ai',
        buildLoginBody('a@b.c', 'secret'),
      );
    } catch (err) {
      caught = err;
    }
    expect(caught).toBeInstanceOf(Sub2ApiError);
    const apiErr = caught as Sub2ApiError;
    expect(apiErr.message).toBe('tencent captcha verification failed');
    expect(apiErr.reason).toBe('TENCENT_CAPTCHA_VERIFICATION_FAILED');
    expect(apiErr.code).toBe(400);
  });

  it('maps captcha and credential errors for toast copy', () => {
    const messages = {
      captchaVerificationFailed: '请先完成验证码',
      loginBadCredentials: '邮箱或密码不正确',
      loginFailed: '登录未完成',
      siteUnreachable: '无法连接该站点',
    };
    expect(
      mapSub2ApiLoginError(
        new Sub2ApiError(
          'tencent captcha verification failed',
          200,
          400,
          'TENCENT_CAPTCHA_VERIFICATION_FAILED',
        ),
        messages,
      ),
    ).toBe('请先完成验证码');
    expect(
      mapSub2ApiLoginError(
        new Sub2ApiError('invalid email or password', 200, 401, 'INVALID_CREDENTIALS'),
        messages,
      ),
    ).toBe('邮箱或密码不正确');
    expect(
      mapSub2ApiLoginError(new Sub2ApiError('rate limited', 200, 429, 'RATE_LIMITED'), messages),
    ).toBe('rate limited');
    expect(mapSub2ApiLoginError(new Error('Failed to fetch'), messages)).toBe('无法连接该站点');
    expect(mapSub2ApiLoginError(new Error('network error: dns'), messages)).toBe('无法连接该站点');
    expect(mapSub2ApiLoginError(new Error('something else'), messages)).toBe('登录未完成');
  });
});
