/**
 * Multi remembered Sub2API credentials.
 *
 * Metadata (site + email + timestamps) lives in localStorage.
 * Passwords live in AgentHub SQLite settings (same DB as Connections API
 * keys) via an injectable vault transport wired by `@/lib/api/sub2api`.
 * This module never imports the tauri backend layer. Never log passwords.
 * UI list helpers never return password values.
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
/** Legacy localStorage vault — migrated once into SQLite then removed. */
const LEGACY_VAULT_KEY = StorageKey.sub2apiRememberedSecrets;
const TOGGLE_KEY = StorageKey.sub2apiRememberEnabled;

export const DEFAULT_SUB2API_REMEMBER_ENABLED = true;

/** Desktop/SQLite vault I/O — wired by the api façade (never import tauri here). */
export type RememberedVaultTransport = {
  get(): Promise<string | null>;
  set(json: string): Promise<void>;
};

let vaultTransport: RememberedVaultTransport | null = null;

/** Wire SQLite (or mock) persistence. Pass null to use memory only. */
export function setRememberedVaultTransport(
  transport: RememberedVaultTransport | null,
): void {
  vaultTransport = transport;
}

/** In-memory vault used by tests and when no transport is configured. */
let memoryVault: PasswordVault = {};
let memoryVaultReady = true;
let hydratePromise: Promise<void> | null = null;

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

function nextLastUsedAt(existing: Sub2ApiRememberedAccountMeta[]): number {
  const now = Date.now();
  let max = 0;
  for (const row of existing) {
    if (row.lastUsedAt > max) max = row.lastUsedAt;
  }
  return Math.max(now, max + 1);
}


function parseVaultJson(raw: string | null | undefined): PasswordVault {
  if (!raw?.trim()) return {};
  try {
    const parsed = JSON.parse(raw) as unknown;
    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) return {};
    const out: PasswordVault = {};
    for (const [id, value] of Object.entries(parsed as Record<string, unknown>)) {
      if (typeof value === 'string' && value.length > 0) out[id] = value;
    }
    return out;
  } catch {
    return {};
  }
}

function readLegacyLocalVault(): PasswordVault {
  const raw = loadJson<PasswordVault | null>(LEGACY_VAULT_KEY, null);
  if (!raw || typeof raw !== 'object' || Array.isArray(raw)) return {};
  const out: PasswordVault = {};
  for (const [id, value] of Object.entries(raw)) {
    if (typeof value === 'string' && value.length > 0) out[id] = value;
  }
  return out;
}

async function persistVault(vault: PasswordVault): Promise<void> {
  memoryVault = { ...vault };
  if (!vaultTransport) return;
  try {
    await vaultTransport.set(JSON.stringify(vault));
  } catch {
    /* desktop unavailable — keep memory only */
  }
}

/**
 * Load password vault from SQLite (Tauri). Migrates legacy localStorage vault
 * once. Safe to call multiple times; concurrent callers share one promise.
 */
export async function hydrateRememberedPasswordVault(): Promise<void> {
  if (hydratePromise) return hydratePromise;
  hydratePromise = (async () => {
    memoryVaultReady = false;
    let vault: PasswordVault = {};
    if (vaultTransport) {
      try {
        const raw = await vaultTransport.get();
        vault = parseVaultJson(raw);
      } catch {
        vault = {};
      }
    }
    const legacy = readLegacyLocalVault();
    if (Object.keys(legacy).length > 0) {
      vault = { ...legacy, ...vault };
      await persistVault(vault);
      removeStorageItem(localStorage, LEGACY_VAULT_KEY);
    } else {
      memoryVault = vault;
    }
    memoryVaultReady = true;
  })();
  try {
    await hydratePromise;
  } finally {
    /* keep hydratePromise so later calls no-op quickly */
  }
}

function ensureVaultSync(): PasswordVault {
  return { ...memoryVault };
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
  const value = ensureVaultSync()[id];
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
 * Persists password to SQLite vault (async fire-and-follow).
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
  const now = nextLastUsedAt(existing);
  const prev = existing.find((row) => row.id === id);
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
  const vault = ensureVaultSync();
  vault[id] = password;
  void persistVault(vault);
  return next;
}

/** Async variant that awaits SQLite persistence (preferred for login success). */
export async function saveRememberedAccountAsync(input: {
  siteUrl: string;
  email: string;
  password: string;
}): Promise<Sub2ApiRememberedAccountMeta | null> {
  if (!isSub2ApiRememberEnabled()) return null;
  const email = input.email.trim();
  const password = input.password;
  if (!email || !password) return null;
  const siteUrl = normalizeSiteUrl(input.siteUrl);
  const id = rememberedAccountId(siteUrl, email);
  const existing = loadMetaList();
  const now = nextLastUsedAt(existing);
  const prev = existing.find((row) => row.id === id);
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
  const vault = ensureVaultSync();
  vault[id] = password;
  await persistVault(vault);
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
  const vault = ensureVaultSync();
  if (id in vault) {
    delete vault[id];
    void persistVault(vault);
  }
}

export async function deleteRememberedAccountAsync(id: string): Promise<void> {
  saveMetaList(loadMetaList().filter((row) => row.id !== id));
  const vault = ensureVaultSync();
  if (id in vault) {
    delete vault[id];
    await persistVault(vault);
  }
}

/** Remove all remembered sets. */
export function clearAllRememberedAccounts(): void {
  removeStorageItem(localStorage, META_KEY);
  removeStorageItem(localStorage, LEGACY_VAULT_KEY);
  void persistVault({});
}

export async function clearAllRememberedAccountsAsync(): Promise<void> {
  removeStorageItem(localStorage, META_KEY);
  removeStorageItem(localStorage, LEGACY_VAULT_KEY);
  await persistVault({});
}

/** Clear passwords only — keep site/email for picker prefills. */
export function clearAllRememberedPasswords(): void {
  removeStorageItem(localStorage, LEGACY_VAULT_KEY);
  void persistVault({});
}

export async function clearAllRememberedPasswordsAsync(): Promise<void> {
  removeStorageItem(localStorage, LEGACY_VAULT_KEY);
  await persistVault({});
}

export function rememberedAccountHasPassword(id: string): boolean {
  return getRememberedPassword(id) != null;
}

/** Test helpers — memory vault only; never writes secrets to fixtures. */
export function __resetRememberedAccountsForTests(): void {
  clearAllRememberedAccounts();
  removeStorageItem(localStorage, TOGGLE_KEY);
  memoryVault = {};
  memoryVaultReady = true;
  hydratePromise = null;
}

export function __setRememberedVaultForTests(vault: PasswordVault): void {
  memoryVault = { ...vault };
  memoryVaultReady = true;
}

export function __isRememberedVaultReady(): boolean {
  return memoryVaultReady;
}
