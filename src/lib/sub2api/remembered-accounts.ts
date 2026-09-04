/**
 * Multi remembered Sub2API credentials.
 *
 * Metadata (site + email + timestamps) lives in localStorage.
 * Passwords live in a separate vault key — same persistence model as the
 * Sub2API JWT session (project: no credential disk encryption / keyring).
 * Never log passwords. UI list helpers never return password values.
 */
import { loadBool, loadJson, saveBool, saveJson } from '@/lib/ui-preferences';
import { removeStorageItem, StorageKey } from '@/lib/storage-key';
import { normalizeSiteUrl } from './url';

export type Sub2ApiRememberedAccountMeta = {
  id: string;
  siteUrl: string;
  email: string;
  lastUsedAt: number;
  createdAt: number;
};

type PasswordVault = Record<string, string>;

const META_KEY = StorageKey.sub2apiRememberedAccounts;
const VAULT_KEY = StorageKey.sub2apiRememberedSecrets;
const TOGGLE_KEY = StorageKey.sub2apiRememberEnabled;

export const DEFAULT_SUB2API_REMEMBER_ENABLED = true;

export function rememberedAccountId(siteUrl: string, email: string): string {
  const site = normalizeSiteUrl(siteUrl);
  const mail = email.trim().toLowerCase();
  return `${site}::${mail}`;
}

export function isSub2ApiRememberEnabled(): boolean {
  return loadBool(TOGGLE_KEY, DEFAULT_SUB2API_REMEMBER_ENABLED);
}

export function setSub2ApiRememberEnabled(enabled: boolean): void {
  saveBool(TOGGLE_KEY, enabled);
}

function loadMetaList(): Sub2ApiRememberedAccountMeta[] {
  const raw = loadJson<Sub2ApiRememberedAccountMeta[] | null>(META_KEY, null);
  if (!Array.isArray(raw)) return [];
  const out: Sub2ApiRememberedAccountMeta[] = [];
  for (const row of raw) {
    if (!row || typeof row !== 'object') continue;
    const email = typeof row.email === 'string' ? row.email.trim() : '';
    const siteRaw = typeof row.siteUrl === 'string' ? row.siteUrl : '';
    if (!email || !siteRaw) continue;
    const siteUrl = normalizeSiteUrl(siteRaw);
    const id =
      typeof row.id === 'string' && row.id.trim()
        ? row.id.trim()
        : rememberedAccountId(siteUrl, email);
    const lastUsedAt =
      typeof row.lastUsedAt === 'number' && Number.isFinite(row.lastUsedAt)
        ? row.lastUsedAt
        : 0;
    const createdAt =
      typeof row.createdAt === 'number' && Number.isFinite(row.createdAt)
        ? row.createdAt
        : lastUsedAt || Date.now();
    out.push({ id, siteUrl, email, lastUsedAt, createdAt });
  }
  return out;
}

function saveMetaList(list: Sub2ApiRememberedAccountMeta[]): void {
  saveJson(META_KEY, list);
}

function loadVault(): PasswordVault {
  const raw = loadJson<PasswordVault | null>(VAULT_KEY, null);
  if (!raw || typeof raw !== 'object' || Array.isArray(raw)) return {};
  const out: PasswordVault = {};
  for (const [id, value] of Object.entries(raw)) {
    if (typeof value === 'string' && value.length > 0) out[id] = value;
  }
  return out;
}

function saveVault(vault: PasswordVault): void {
  if (Object.keys(vault).length === 0) {
    removeStorageItem(localStorage, VAULT_KEY);
    return;
  }
  saveJson(VAULT_KEY, vault);
}

/** Accounts for UI — sorted by last used, never includes passwords. */
export function listRememberedAccounts(): Sub2ApiRememberedAccountMeta[] {
  return [...loadMetaList()].sort((a, b) => b.lastUsedAt - a.lastUsedAt);
}

export function getLastUsedRememberedAccount(): Sub2ApiRememberedAccountMeta | null {
  const list = listRememberedAccounts();
  return list[0] ?? null;
}

export function getRememberedPassword(id: string): string | null {
  const vault = loadVault();
  const value = vault[id];
  return typeof value === 'string' && value.length > 0 ? value : null;
}

/** Prefill payload including password when present in the vault. */
export function loadRememberedCredentials(id: string): {
  siteUrl: string;
  email: string;
  password: string;
} | null {
  const meta = loadMetaList().find((row) => row.id === id);
  if (!meta) return null;
  return {
    siteUrl: meta.siteUrl,
    email: meta.email,
    password: getRememberedPassword(id) ?? '',
  };
}

/**
 * Upsert after successful login when remember is ON.
 * Never called with logging of `password`.
 */
export function saveRememberedAccount(input: {
  siteUrl: string;
  email: string;
  password: string;
}): Sub2ApiRememberedAccountMeta | null {
  if (!isSub2ApiRememberEnabled()) return null;
  const email = input.email.trim();
  const password = input.password;
  if (!email || !password) return null;
  const siteUrl = normalizeSiteUrl(input.siteUrl);
  const id = rememberedAccountId(siteUrl, email);
  const existing = loadMetaList();
  const prev = existing.find((row) => row.id === id);
  const maxUsed = existing.reduce((m, row) => Math.max(m, row.lastUsedAt || 0), 0);
  const now = Math.max(Date.now(), maxUsed + 1);
  const list = existing.filter((row) => row.id !== id);
  const next: Sub2ApiRememberedAccountMeta = {
    id,
    siteUrl,
    email,
    lastUsedAt: now,
    createdAt: prev?.createdAt ?? now,
  };
  list.push(next);
  saveMetaList(list);
  const vault = loadVault();
  vault[id] = password;
  saveVault(vault);
  return next;
}

export function touchRememberedAccount(id: string): void {
  const list = loadMetaList();
  const idx = list.findIndex((row) => row.id === id);
  if (idx < 0) return;
  list[idx] = { ...list[idx], lastUsedAt: Date.now() };
  saveMetaList(list);
}

/** Delete one set (meta + password). */
export function deleteRememberedAccount(id: string): void {
  saveMetaList(loadMetaList().filter((row) => row.id !== id));
  const vault = loadVault();
  if (id in vault) {
    delete vault[id];
    saveVault(vault);
  }
}

/** Remove all remembered sets. */
export function clearAllRememberedAccounts(): void {
  removeStorageItem(localStorage, META_KEY);
  removeStorageItem(localStorage, VAULT_KEY);
}

/** Clear passwords only — keep site/email for picker prefills. */
export function clearAllRememberedPasswords(): void {
  removeStorageItem(localStorage, VAULT_KEY);
}

export function rememberedAccountHasPassword(id: string): boolean {
  return getRememberedPassword(id) != null;
}

/** Test helpers */
export function __resetRememberedAccountsForTests(): void {
  clearAllRememberedAccounts();
  removeStorageItem(localStorage, TOGGLE_KEY);
}
