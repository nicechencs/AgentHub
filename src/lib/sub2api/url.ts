import type { Sub2ApiPublicSettings } from './types';
import { SUB2API_DEFAULT_SITE_URL } from './types';

export { SUB2API_DEFAULT_SITE_URL };

export function normalizeSiteUrl(raw: string): string {
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
  if (/\/api\/v1$/i.test(root)) return root;
  return `${root}/api/v1`;
}

/** Gateway base for Connections: public api_base_url, else instance root. */
export function sub2apiGatewayBaseUrl(
  siteUrl: string,
  publicSettings?: Sub2ApiPublicSettings | null,
): string {
  const fromPublic =
    typeof publicSettings?.api_base_url === 'string'
      ? publicSettings.api_base_url.trim()
      : '';
  if (fromPublic) return normalizeSiteUrl(fromPublic);
  return normalizeSiteUrl(siteUrl);
}

export function maskApiKey(key: string): string {
  const value = key.trim();
  if (!value) return '••••';
  if (value.length <= 8) return `${value.slice(0, 2)}…`;
  return `${value.slice(0, 4)}…${value.slice(-4)}`;
}
