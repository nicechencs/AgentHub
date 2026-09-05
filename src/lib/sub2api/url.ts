import type { Sub2ApiPublicSettings } from './types';
import { SUB2API_DEFAULT_SITE_URL } from './types';

export { SUB2API_DEFAULT_SITE_URL };

export type NormalizeSiteUrlOk = {
  ok: true;
  /** scheme + host + optional port only */
  url: string;
  /** True when a non-root path, query, or hash was removed from the paste. */
  stripped: boolean;
};

export type NormalizeSiteUrlErr = {
  ok: false;
  reason: 'empty' | 'invalid';
};

export type NormalizeSiteUrlResult = NormalizeSiteUrlOk | NormalizeSiteUrlErr;

/**
 * Parse a pasted site URL into scheme+host[+port].
 * Strips `/login`, `/api/v1`, and any other path/query/hash.
 * Does not fall back to the default — callers decide.
 */
export function tryNormalizeSiteUrl(raw: string): NormalizeSiteUrlResult {
  const trimmed = raw.trim();
  if (!trimmed) return { ok: false, reason: 'empty' };
  try {
    const withScheme = trimmed.includes('://') ? trimmed : `https://${trimmed}`;
    const url = new URL(withScheme);
    if (url.protocol !== 'http:' && url.protocol !== 'https:') {
      return { ok: false, reason: 'invalid' };
    }
    if (!url.hostname) return { ok: false, reason: 'invalid' };
    const path = url.pathname.replace(/\/+$/, '');
    const stripped =
      (path !== '' && path !== '/')
      || Boolean(url.search)
      || Boolean(url.hash);
    return { ok: true, url: url.origin, stripped };
  } catch {
    return { ok: false, reason: 'invalid' };
  }
}

/**
 * Site root for login / API: scheme+host[+port].
 * Empty or invalid input → default site (legacy callers / session load).
 */
export function normalizeSiteUrl(raw: string): string {
  const result = tryNormalizeSiteUrl(raw);
  if (result.ok) return result.url;
  return SUB2API_DEFAULT_SITE_URL;
}

/**
 * HTTP base that may keep a path (e.g. public `api_base_url` with `/v1`).
 * Trailing slashes removed; query/hash dropped.
 */
export function normalizeHttpBaseUrl(raw: string): string {
  const trimmed = raw.trim().replace(/\/+$/, '');
  if (!trimmed) return SUB2API_DEFAULT_SITE_URL;
  try {
    const url = new URL(trimmed.includes('://') ? trimmed : `https://${trimmed}`);
    if (url.protocol !== 'http:' && url.protocol !== 'https:') {
      return SUB2API_DEFAULT_SITE_URL;
    }
    const path = url.pathname.replace(/\/+$/, '');
    return path && path !== '/' ? `${url.origin}${path}` : url.origin;
  } catch {
    return SUB2API_DEFAULT_SITE_URL;
  }
}

export function sub2apiLoginUrl(siteUrl: string): string {
  return `${normalizeSiteUrl(siteUrl)}/login`;
}

export function sub2apiApiRoot(siteUrl: string): string {
  const root = normalizeSiteUrl(siteUrl);
  return `${root}/api/v1`;
}

/** Gateway base for Connections: public api_base_url (path kept), else instance root. */
export function sub2apiGatewayBaseUrl(
  siteUrl: string,
  publicSettings?: Sub2ApiPublicSettings | null,
): string {
  const fromPublic =
    typeof publicSettings?.api_base_url === 'string'
      ? publicSettings.api_base_url.trim()
      : '';
  if (fromPublic) return normalizeHttpBaseUrl(fromPublic);
  return normalizeSiteUrl(siteUrl);
}

export function maskApiKey(key: string): string {
  const value = key.trim();
  if (!value) return '••••';
  if (value.length <= 8) return `${value.slice(0, 2)}…`;
  return `${value.slice(0, 4)}…${value.slice(-4)}`;
}

/** Mask email for account picker rows — never full local-part in lists. */
export function maskEmail(email: string): string {
  const trimmed = email.trim();
  const at = trimmed.indexOf('@');
  if (at <= 0) return '***';
  const local = trimmed.slice(0, at);
  const domain = trimmed.slice(at + 1);
  if (!domain) return '***';
  if (local.length <= 1) return `${local}***@${domain}`;
  if (local.length === 2) return `${local[0]}***@${domain}`;
  return `${local.slice(0, 2)}***@${domain}`;
}
