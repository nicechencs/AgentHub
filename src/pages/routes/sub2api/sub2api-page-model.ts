/** Pure helpers for the Sub2API routes page. */

import type { Sub2ApiKey, Sub2ApiSession, Sub2ApiUser } from '@/lib/sub2api';
import { SUB2API_DEFAULT_SITE_URL, normalizeSiteUrl } from '@/lib/sub2api';

export type Sub2ApiPagePhase = 'logged-out' | 'logging-in' | 'logged-in';

export function sub2apiPagePhase(
  session: Sub2ApiSession | null,
  loggingIn: boolean,
): Sub2ApiPagePhase {
  if (loggingIn) return 'logging-in';
  if (session?.accessToken) return 'logged-in';
  return 'logged-out';
}

export function sub2apiDisplayName(
  user: Sub2ApiUser | null | undefined,
  session?: Sub2ApiSession | null,
): string {
  if (user) {
    const name = (user.display_name || user.username || user.email || '').trim();
    if (name) return name;
  }
  return (session?.user?.email || '').trim();
}

export function sub2apiKeyStatusLabel(
  status: string,
  labels: { active: string; other: string },
): string {
  return status === 'active' ? labels.active : labels.other;
}

export function initialSiteUrlDraft(session: Sub2ApiSession | null): string {
  return session?.siteUrl || SUB2API_DEFAULT_SITE_URL;
}

export function prepareSiteUrlForLogin(raw: string): string {
  return normalizeSiteUrl(raw || SUB2API_DEFAULT_SITE_URL);
}

export function sortSub2ApiKeys(keys: readonly Sub2ApiKey[]): Sub2ApiKey[] {
  return [...keys].sort((a, b) => {
    const an = (a.name || '').localeCompare(b.name || '', undefined, { sensitivity: 'base' });
    if (an !== 0) return an;
    return a.id - b.id;
  });
}
