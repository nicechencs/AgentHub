/**
 * Sub2API HTTP client — Bearer JWT, envelope `{ code, message, data }`.
 * Never logs tokens or keys.
 */
import type {
  Sub2ApiAuthContext,
  Sub2ApiEnvelope,
  Sub2ApiKey,
  Sub2ApiKeyList,
  Sub2ApiPublicSettings,
  Sub2ApiUser,
} from './types';
import { sub2apiApiRoot } from './url';

export class Sub2ApiError extends Error {
  readonly status: number;
  readonly code: number;

  constructor(message: string, status: number, code: number) {
    super(message);
    this.name = 'Sub2ApiError';
    this.status = status;
    this.code = code;
  }
}

async function parseEnvelope<T>(response: Response): Promise<T> {
  let body: Sub2ApiEnvelope<T> | null = null;
  try {
    body = (await response.json()) as Sub2ApiEnvelope<T>;
  } catch {
    throw new Sub2ApiError(
      response.ok ? 'Invalid response' : `HTTP ${response.status}`,
      response.status,
      -1,
    );
  }
  if (!body || typeof body !== 'object' || !('code' in body)) {
    throw new Sub2ApiError('Invalid response envelope', response.status, -1);
  }
  if (body.code !== 0) {
    throw new Sub2ApiError(body.message || 'Request failed', response.status, body.code);
  }
  return body.data;
}

async function request<T>(
  siteUrl: string,
  path: string,
  init: RequestInit & { accessToken?: string | null } = {},
): Promise<T> {
  const headers = new Headers(init.headers);
  if (!headers.has('Content-Type') && init.body) {
    headers.set('Content-Type', 'application/json');
  }
  const token = init.accessToken?.trim();
  if (token) headers.set('Authorization', `Bearer ${token}`);
  const { accessToken: _drop, ...rest } = init;
  const response = await fetch(`${sub2apiApiRoot(siteUrl)}${path}`, {
    ...rest,
    headers,
  });
  return parseEnvelope<T>(response);
}

export function fetchPublicSettings(input: { siteUrl: string }): Promise<Sub2ApiPublicSettings> {
  return request<Sub2ApiPublicSettings>(input.siteUrl, '/settings/public');
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
