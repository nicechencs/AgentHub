/**
 * Sub2API HTTP client — Bearer JWT, envelope `{ code, message, data }`.
 * Desktop (Tauri) uses Rust `sub2api_http_request` to bypass WebView CORS.
 * Browser/vitest keeps `fetch`. Never logs tokens, passwords, or keys.
 */
import type {
  Sub2ApiAuthContext,
  Sub2ApiAuthTokens,
  Sub2ApiCaptchaKind,
  Sub2ApiCaptchaProof,
  Sub2ApiEnvelope,
  Sub2ApiKey,
  Sub2ApiKeyList,
  Sub2ApiLogin2FARequest,
  Sub2ApiLoginRequest,
  Sub2ApiLoginResult,
  Sub2ApiPublicSettings,
  Sub2ApiUser,
} from './types';
import { sub2apiApiRoot } from './url';
import { isTauriApp } from '@/lib/platform';

export class Sub2ApiError extends Error {
  readonly status: number;
  readonly code: number;
  readonly reason?: string;

  constructor(message: string, status: number, code: number, reason?: string) {
    super(message);
    this.name = 'Sub2ApiError';
    this.status = status;
    this.code = code;
    if (reason) this.reason = reason;
  }
}

/** Thrown when the site cannot be reached (CORS/network/transport). */
export class Sub2ApiNetworkError extends Error {
  constructor(message = 'network error') {
    super(message);
    this.name = 'Sub2ApiNetworkError';
  }
}

export type Sub2ApiLoginErrorMessages = {
  captchaVerificationFailed: string;
  loginBadCredentials?: string;
  loginFailed: string;
  /** Prefer siteProbeFailed / network unreachable copy. */
  siteUnreachable?: string;
};

function looksLikeNetworkFailure(err: unknown): boolean {
  if (err instanceof Sub2ApiNetworkError) return true;
  const msg = err instanceof Error ? err.message : String(err ?? '');
  return /failed to fetch|network error|networkerror|load failed|cors|unreachable|timed out|timeout|connection refused|name not resolved|dns|econnrefused|enotfound/i.test(
    msg,
  );
}

/** Map API login/2FA errors to user-facing copy. Never logs secrets. */
export function mapSub2ApiLoginError(
  err: unknown,
  messages: Sub2ApiLoginErrorMessages,
): string {
  if (looksLikeNetworkFailure(err)) {
    return messages.siteUnreachable?.trim() || messages.loginFailed;
  }
  if (!(err instanceof Sub2ApiError)) return messages.loginFailed;
  const reason = (err.reason ?? '').toUpperCase();
  const msg = err.message || '';
  if (reason.includes('CAPTCHA') || /captcha/i.test(msg)) {
    return messages.captchaVerificationFailed;
  }
  if (
    reason.includes('INVALID_CREDENTIAL')
    || reason.includes('INVALID_PASSWORD')
    || reason.includes('WRONG_PASSWORD')
    || reason.includes('BAD_CREDENTIAL')
    || reason.includes('UNAUTHORIZED')
    || /invalid.*(password|credential|email)|wrong password|incorrect (password|email)/i.test(msg)
  ) {
    return messages.loginBadCredentials?.trim() || err.message || messages.loginFailed;
  }
  const trimmed = err.message?.trim();
  return trimmed || messages.loginFailed;
}

type RawHttpResult = { status: number; bodyText: string };

async function desktopHttp(input: {
  method: string;
  url: string;
  headers: Record<string, string>;
  body?: string;
}): Promise<RawHttpResult> {
  const { invoke } = await import('@/lib/backend/tauri/invoke');
  try {
    const raw = await invoke<{ status: number; body: string }>('sub2api_http_request', {
      method: input.method,
      url: input.url,
      headers: input.headers,
      body: input.body ?? null,
    });
    return { status: raw.status, bodyText: raw.body ?? '' };
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err ?? 'network error');
    throw new Sub2ApiNetworkError(msg);
  }
}

async function browserHttp(input: {
  method: string;
  url: string;
  headers: Record<string, string>;
  body?: string;
}): Promise<RawHttpResult> {
  try {
    const response = await fetch(input.url, {
      method: input.method,
      headers: input.headers,
      body: input.body,
    });
    const bodyText = await response.text();
    return { status: response.status, bodyText };
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err ?? 'network error');
    throw new Sub2ApiNetworkError(msg);
  }
}

function parseEnvelopeText<T>(status: number, bodyText: string): T {
  let body: Sub2ApiEnvelope<T> | null = null;
  try {
    body = JSON.parse(bodyText) as Sub2ApiEnvelope<T>;
  } catch {
    throw new Sub2ApiError(
      status >= 200 && status < 300 ? 'Invalid response' : `HTTP ${status}`,
      status,
      -1,
    );
  }
  if (!body || typeof body !== 'object' || !('code' in body)) {
    throw new Sub2ApiError('Invalid response envelope', status, -1);
  }
  if (body.code !== 0) {
    const reason = typeof body.reason === 'string' && body.reason.trim()
      ? body.reason.trim()
      : undefined;
    throw new Sub2ApiError(
      body.message || 'Request failed',
      status,
      body.code,
      reason,
    );
  }
  return body.data;
}

async function request<T>(
  siteUrl: string,
  path: string,
  init: RequestInit & { accessToken?: string | null } = {},
): Promise<T> {
  const headers: Record<string, string> = {};
  if (init.headers) {
    const h = new Headers(init.headers);
    h.forEach((value, key) => {
      headers[key] = value;
    });
  }
  const body =
    typeof init.body === 'string'
      ? init.body
      : init.body != null
        ? String(init.body)
        : undefined;
  if (!headers['Content-Type'] && !headers['content-type'] && body) {
    headers['Content-Type'] = 'application/json';
  }
  const token = init.accessToken?.trim();
  if (token) headers.Authorization = `Bearer ${token}`;
  const method = (init.method || 'GET').toUpperCase();
  const url = `${sub2apiApiRoot(siteUrl)}${path}`;
  const raw = isTauriApp()
    ? await desktopHttp({ method, url, headers, body })
    : await browserHttp({ method, url, headers, body });
  return parseEnvelopeText<T>(raw.status, raw.bodyText);
}

export function fetchPublicSettings(input: { siteUrl: string }): Promise<Sub2ApiPublicSettings> {
  return request<Sub2ApiPublicSettings>(input.siteUrl, '/settings/public');
}

export function isTotp2FARequired(
  response: Sub2ApiLoginResult,
): response is Extract<Sub2ApiLoginResult, { requires_2fa: true }> {
  return (
    typeof response === 'object'
    && response !== null
    && 'requires_2fa' in response
    && response.requires_2fa === true
  );
}

export function resolveCaptchaKind(settings: Sub2ApiPublicSettings | null | undefined): Sub2ApiCaptchaKind {
  if (!settings) return 'none';
  if (settings.turnstile_enabled && settings.turnstile_site_key?.trim()) return 'turnstile';
  if (settings.tencent_captcha_enabled && settings.tencent_captcha_app_id?.trim()) return 'tencent';
  if (
    settings.aliyun_captcha_enabled
    && settings.aliyun_captcha_scene_id?.trim()
    && settings.aliyun_captcha_prefix?.trim()
  ) {
    return 'aliyun';
  }
  return 'none';
}

/** Merge password login body with optional captcha proof. Never include empty tokens. */
export function buildLoginBody(
  email: string,
  password: string,
  proof?: Sub2ApiCaptchaProof | null,
): Sub2ApiLoginRequest {
  const body: Sub2ApiLoginRequest = {
    email: email.trim(),
    password,
  };
  const turnstile = proof?.turnstile_token?.trim();
  if (turnstile) body.turnstile_token = turnstile;
  const ticket = proof?.tencent_captcha_ticket?.trim();
  const randstr = proof?.tencent_captcha_randstr?.trim();
  if (ticket && randstr) {
    body.tencent_captcha_ticket = ticket;
    body.tencent_captcha_randstr = randstr;
  }
  return body;
}

export function loginWithPassword(
  siteUrl: string,
  body: Sub2ApiLoginRequest,
): Promise<Sub2ApiLoginResult> {
  return request<Sub2ApiLoginResult>(siteUrl, '/auth/login', {
    method: 'POST',
    body: JSON.stringify(body),
  });
}

export function loginWith2FA(
  siteUrl: string,
  body: Sub2ApiLogin2FARequest,
): Promise<Sub2ApiAuthTokens> {
  return request<Sub2ApiAuthTokens>(siteUrl, '/auth/login/2fa', {
    method: 'POST',
    body: JSON.stringify({
      temp_token: body.temp_token.trim(),
      totp_code: body.totp_code.trim(),
    }),
  });
}

export function fetchCurrentUser(ctx: Sub2ApiAuthContext): Promise<Sub2ApiUser> {
  return request<Sub2ApiUser>(ctx.siteUrl, '/auth/me', { accessToken: ctx.accessToken });
}

export async function listApiKeys(
  ctx: Sub2ApiAuthContext,
  page = 1,
  pageSize = 50,
): Promise<Sub2ApiKeyList> {
  return request<Sub2ApiKeyList>(ctx.siteUrl, `/keys?page=${page}&page_size=${pageSize}`, {
    accessToken: ctx.accessToken,
  });
}

export function createApiKey(ctx: Sub2ApiAuthContext, name: string): Promise<Sub2ApiKey> {
  return request<Sub2ApiKey>(ctx.siteUrl, '/keys', {
    method: 'POST',
    body: JSON.stringify({ name: name.trim() || 'AgentHub' }),
    accessToken: ctx.accessToken,
  });
}

export function refreshAuthTokens(
  ctx: Sub2ApiAuthContext,
  refreshToken: string,
): Promise<{ access_token: string; refresh_token?: string; expires_in?: number }> {
  return request(ctx.siteUrl, '/auth/refresh', {
    method: 'POST',
    body: JSON.stringify({ refresh_token: refreshToken }),
  });
}

export async function logoutRemote(
  ctx: Sub2ApiAuthContext,
  refreshToken?: string | null,
): Promise<void> {
  try {
    await request(ctx.siteUrl, '/auth/logout', {
      method: 'POST',
      body: JSON.stringify(refreshToken ? { refresh_token: refreshToken } : {}),
      accessToken: ctx.accessToken,
    });
  } catch {
    /* still clear local session */
  }
}

export { maskApiKey } from './url';

/** Prefer keys that expose a non-empty secret for sync. */
export function selectableSub2ApiKeys(keys: readonly Sub2ApiKey[]): Sub2ApiKey[] {
  return keys.filter((k) => typeof k.key === 'string' && k.key.trim().length > 0);
}
