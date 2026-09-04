/**
 * Remembered Sub2API site URLs for the login picker.
 * LocalStorage only — no passwords. Distinct from remembered accounts.
 */
import { loadJson, saveJson } from '@/lib/ui-preferences';
import { readStorageItem, removeStorageItem, StorageKey } from '@/lib/storage-key';
import { tryNormalizeSiteUrl } from './url';

export type Sub2ApiRememberedSite = {
  siteUrl: string;
  lastUsedAt: number;
};

const SITES_KEY = StorageKey.sub2apiRememberedSites;
const MAX_SITES = 20;

function parseSiteList(raw: unknown): Sub2ApiRememberedSite[] {
  if (!Array.isArray(raw)) return [];
  const out: Sub2ApiRememberedSite[] = [];
  const seen = new Set<string>();
  for (const row of raw) {
    if (typeof row === 'string') {
      const parsed = tryNormalizeSiteUrl(row);
      if (!parsed.ok || seen.has(parsed.url)) continue;
      seen.add(parsed.url);
      out.push({ siteUrl: parsed.url, lastUsedAt: 0 });
      continue;
    }
    if (!row || typeof row !== 'object') continue;
    const siteRaw = typeof (row as Sub2ApiRememberedSite).siteUrl === 'string'
      ? (row as Sub2ApiRememberedSite).siteUrl
      : '';
    const parsed = tryNormalizeSiteUrl(siteRaw);
    if (!parsed.ok || seen.has(parsed.url)) continue;
    seen.add(parsed.url);
    const lastUsedAt =
      typeof (row as Sub2ApiRememberedSite).lastUsedAt === 'number'
      && Number.isFinite((row as Sub2ApiRememberedSite).lastUsedAt)
        ? (row as Sub2ApiRememberedSite).lastUsedAt
        : 0;
    out.push({ siteUrl: parsed.url, lastUsedAt });
  }
  return out;
}

function loadSiteList(): Sub2ApiRememberedSite[] {
  return parseSiteList(loadJson<unknown>(SITES_KEY, null));
}

function saveSiteList(list: Sub2ApiRememberedSite[]): void {
  const trimmed = [...list]
    .sort((a, b) => b.lastUsedAt - a.lastUsedAt)
    .slice(0, MAX_SITES);
  saveJson(SITES_KEY, trimmed);
}

function nextLastUsedAt(existing: Sub2ApiRememberedSite[]): number {
  const now = Date.now();
  let max = 0;
  for (const row of existing) {
    if (row.lastUsedAt > max) max = row.lastUsedAt;
  }
  return Math.max(now, max + 1);
}

/** Unique site URLs, last used first. */
export function listRememberedSites(): string[] {
  return [...loadSiteList()]
    .sort((a, b) => b.lastUsedAt - a.lastUsedAt)
    .map((row) => row.siteUrl);
}

/** Upsert a valid site. Returns the normalized URL, or null if invalid. */
export function saveRememberedSite(raw: string): string | null {
  const parsed = tryNormalizeSiteUrl(raw);
  if (!parsed.ok) return null;
  const existing = loadSiteList();
  const lastUsedAt = nextLastUsedAt(existing);
  const list = existing.filter((row) => row.siteUrl !== parsed.url);
  list.push({ siteUrl: parsed.url, lastUsedAt });
  saveSiteList(list);
  return parsed.url;
}

export function deleteRememberedSite(raw: string): void {
  const parsed = tryNormalizeSiteUrl(raw);
  const target = parsed.ok ? parsed.url : raw.trim();
  if (!target) return;
  saveSiteList(loadSiteList().filter((row) => row.siteUrl !== target));
}

/**
 * First-run seed from known URLs (accounts / last session).
 * No-op once the sites key has been written, including an empty list.
 */
export function seedRememberedSitesIfUnset(urls: readonly string[]): void {
  if (readStorageItem(localStorage, SITES_KEY) != null) return;
  for (const url of urls) {
    saveRememberedSite(url);
  }
}

export function clearRememberedSites(): void {
  removeStorageItem(localStorage, SITES_KEY);
}

/** Test helper. */
export function __resetRememberedSitesForTests(): void {
  clearRememberedSites();
}
