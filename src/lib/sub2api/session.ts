/**
 * Sub2API JWT session — localStorage only (project: no credential disk encryption).
 * Long-term product state = gateway API Key in Connections.
 * Logout clears session only; remembered accounts are separate.
 */
import { loadJson, saveJson, StorageKey } from '@/lib/ui-preferences';
import { removeStorageItem } from '@/lib/storage-key';
import type { Sub2ApiSession, Sub2ApiUser } from './types';
import { normalizeHttpBaseUrl, normalizeSiteUrl, sub2apiGatewayBaseUrl } from './url';

const SESSION_KEY = StorageKey.sub2apiSession;

/** Refresh when within this window of expiry (or already expired). */
const REFRESH_SKEW_MS = 60_000;

export function loadSub2ApiSession(): Sub2ApiSession | null {
  const raw = loadJson<Sub2ApiSession | null>(SESSION_KEY, null);
  if (!raw?.accessToken?.trim() || !raw.siteUrl?.trim()) return null;
  const siteUrl = normalizeSiteUrl(raw.siteUrl);
  return {
    ...raw,
    siteUrl,
    gatewayBaseUrl: normalizeHttpBaseUrl(raw.gatewayBaseUrl || siteUrl),
    accessToken: raw.accessToken.trim(),
    refreshToken: raw.refreshToken?.trim() || undefined,
  };
}

export function saveSub2ApiSession(session: Sub2ApiSession): void {
  const siteUrl = normalizeSiteUrl(session.siteUrl);
  const next: Sub2ApiSession = {
    ...session,
    siteUrl,
    gatewayBaseUrl: normalizeHttpBaseUrl(session.gatewayBaseUrl || siteUrl),
    accessToken: session.accessToken.trim(),
    refreshToken: session.refreshToken?.trim() || undefined,
  };
  saveJson(SESSION_KEY, next);
}

export function clearSub2ApiSession(): void {
  removeStorageItem(localStorage, SESSION_KEY);
}

export function sessionFromTokens(input: {
  siteUrl: string;
  gatewayBaseUrl?: string;
  accessToken: string;
  refreshToken?: string;
  expiresAt?: number;
  expiresIn?: number;
  user?: Sub2ApiUser | null;
}): Sub2ApiSession {
  const siteUrl = normalizeSiteUrl(input.siteUrl);
  const expiresAt =
    input.expiresAt
    ?? (typeof input.expiresIn === 'number' && input.expiresIn > 0
      ? Date.now() + input.expiresIn * 1000
      : undefined);
  return {
    siteUrl,
    gatewayBaseUrl: normalizeHttpBaseUrl(
      input.gatewayBaseUrl || sub2apiGatewayBaseUrl(siteUrl),
    ),
    accessToken: input.accessToken.trim(),
    refreshToken: input.refreshToken?.trim() || undefined,
    expiresAt,
    user: input.user ?? null,
  };
}

/** True when access token should be refreshed soon (or has no expiry but we still try later). */
export function sessionNeedsRefresh(
  session: Sub2ApiSession,
  now = Date.now(),
): boolean {
  if (!session.refreshToken) return false;
  if (typeof session.expiresAt !== 'number' || !Number.isFinite(session.expiresAt)) {
    return false;
  }
  return session.expiresAt <= now + REFRESH_SKEW_MS;
}

export function __setSub2ApiSessionForTests(session: Sub2ApiSession | null): void {
  if (session) saveSub2ApiSession(session);
  else clearSub2ApiSession();
}
