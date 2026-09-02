import { extractAccountCredentialFiles } from '@/lib/credential-files';
import type { Account, AccountKind, AgentKey } from '@/lib/types';
import type { LiveAuthProbe } from './account-port';
import { normalizeAuthHealth, type AuthHealth } from './auth-state';

export interface CoreAccount {
  id: string;
  agentId: AgentKey;
  kind: AccountKind;
  label: string;
  credentials?: Record<string, unknown>;
  extra?: Record<string, unknown>;
  status: string;
  /** Optional semantic auth fields added by newer backends. */
  health?: string | null;
  source?: string | null;
  authSource?: string | null;
  liveRevision?: string | null;
  isCurrent: boolean;
  createdAt: string;
  updatedAt: string;
}

export interface CoreAccountSwitchResult {
  account: CoreAccount;
  backup?: { id: string } | null;
  backfilledAccountId?: string | null;
}

export function mapCoreAccount(a: CoreAccount): Account {
  const extra = a.extra ?? {};
  const credentials = a.credentials ?? {};
  const email =
    pickString(extra.email) ??
    pickString(credentials.email) ??
    pickString(credentials.email_address) ??
    pickString(credentials.emailAddress);
  const provider =
    pickString(credentials.provider_name) ??
    pickString(credentials.name) ??
    pickString(extra.provider) ??
    pickString(credentials.provider) ??
    inferProviderFromBody(credentials.body);
  const subjectId =
    pickString(extra.sub) ??
    pickString(credentials.sub) ??
    pickString(credentials.subject) ??
    pickString(credentials.principal_id) ??
    pickString(credentials.account_id);
  const rawIdentity = pickString(extra.identityLabel);
  const identityLabel =
    email ??
    (rawIdentity && looksLikeEmail(rawIdentity) ? rawIdentity : undefined) ??
    (rawIdentity && !looksLikeUuid(rawIdentity) ? rawIdentity : undefined) ??
    (subjectId ? shortId(subjectId) : undefined) ??
    (rawIdentity && !looksLikeUuid(rawIdentity) ? rawIdentity : undefined) ??
    a.label;
  const subscription =
    pickString(extra.subscription) ??
    pickString(credentials.plan_type) ??
    pickString(credentials.planType) ??
    pickString(credentials.subscription_tier);
  let tokenRemainingSec =
    typeof extra.tokenRemainingSec === 'number' ? extra.tokenRemainingSec : undefined;
  // Derive remaining from expiresAt when adapter only stored absolute expiry.
  if (tokenRemainingSec === undefined) {
    tokenRemainingSec = remainingSecFromExpiresAt(extra.expiresAt);
  }
  const quota5hPct = typeof extra.quota5hPct === 'number' ? extra.quota5hPct : undefined;
  const quota7dPct = typeof extra.quota7dPct === 'number' ? extra.quota7dPct : undefined;
  // Live countdown from absolute resets. Keep 5h and 7d separate — never mix.
  const rem5 = remainingSecFromExpiresAt(extra.quota5hResetAt);
  const rem7 = remainingSecFromExpiresAt(extra.quota7dResetAt);
  const quotaResetIn =
    formatQuotaResetIn(
      rem5 !== undefined ? Math.min(Math.max(rem5, 0), 5 * 3600 + 120) : undefined,
    ) ?? (typeof extra.quotaResetIn === 'string' ? extra.quotaResetIn : undefined);
  const quota7dResetIn =
    formatQuotaResetIn(
      rem7 !== undefined ? Math.min(Math.max(rem7, 0), 7 * 24 * 3600 + 120) : undefined,
    ) ?? (typeof extra.quota7dResetIn === 'string' ? extra.quota7dResetIn : undefined);
  const lastUsedAt =
    typeof extra.lastUsedAt === 'string'
      ? extra.lastUsedAt
      : a.updatedAt
        ? new Date(a.updatedAt.replace(' ', 'T') + 'Z').toISOString()
        : undefined;

  const credentialFormat =
    typeof credentials.format === 'string' ? credentials.format : undefined;
  const envKey = typeof credentials.env_key === 'string' ? credentials.env_key : undefined;
  const source =
    typeof a.source === 'string'
      ? a.source
      : typeof extra.source === 'string'
      ? extra.source
      : typeof credentials.source === 'string'
        ? credentials.source
        : undefined;
  const credentialSummary = buildCredentialSummary(credentials, {
    format: credentialFormat,
    envKey,
    source,
    provider,
  });
  // `health`/`extra.health` belong to this saved pool row. The newer
  // `extra.auth*` fields describe the agent's current live configuration and
  // must remain separate so a stale pool account cannot overwrite a probe.
  const poolAuthHealth = normalizeAuthHealth(a.health ?? extra.health);
  const liveAuthHealth = normalizeAuthHealth(extra.authHealth);
  const liveAuthSource =
    pickString(a.authSource) ??
    pickString(extra.authSource);
  const liveAuthRevision =
    pickString(a.liveRevision) ??
    pickString(extra.liveRevision);

  const recoveredSecretTail =
    pickString(extra.secretTail) ??
    secretTailFromMaskedPreview(rawIdentity) ??
    secretTailFromMaskedPreview(a.label);

  const tokenExpired =
    extra.tokenExpired === true ||
    (tokenRemainingSec !== undefined && tokenRemainingSec <= 0);
  const refreshable =
    a.kind === 'oauth' && hasNonEmptyField(credentials, [
      'refresh_token',
      'refreshToken',
      'refresh',
    ]);
  const tokenValid =
    (a.status === 'active' || a.status === '') && (!tokenExpired || refreshable);

  // Prefer real account identity as the list title when available.
  // Backend may still store placeholder labels (codex-oauth / grok-oauth) even after
  // email was written into extra — always prefer email for display.
  const label =
    (email && looksLikeEmail(email) ? email : undefined) ??
    (identityLabel && looksLikeEmail(identityLabel) ? identityLabel : undefined) ??
    improveGenericOAuthLabel(a.label, {
      provider,
      identityLabel,
      email,
      subjectId,
      agentId: a.agentId,
    }) ??
    improveGenericApiKeyLabel(a.label, provider, a.agentId) ??
    a.label;

  return {
    id: a.id,
    agentId: a.agentId,
    kind: a.kind,
    label,
    email,
    identityLabel,
    provider,
    subjectId,
    subscription,
    isCurrent: a.isCurrent,
    tokenValid,
    // Keep the legacy account field populated when old callers do not yet
    // consume liveAuthHealth, while preserving its separate provenance.
    authHealth: poolAuthHealth ?? liveAuthHealth,
    liveAuthHealth,
    liveAuthSource,
    liveAuthRevision,
    refreshable,
    status: a.status || undefined,
    tokenRemainingSec,
    quota5hPct,
    quota7dPct,
    quotaResetIn,
    quota7dResetIn,
    lastUsedAt,
    updatedAt: a.updatedAt,
    createdAt: a.createdAt,
    credentialFormat,
    source,
    envKey,
    credentialSummary,
    credentialFiles: extractAccountCredentialFiles({
      agentId: a.agentId,
      kind: a.kind,
      credentials,
      source,
      format: credentialFormat,
    }),
    refreshTokenPreview: a.kind === 'oauth' ? pickString(extra.refreshTokenPreview) : undefined,
    secretTail: recoveredSecretTail,
    secretHash: pickString(extra.secretHash),
    home: extra.home === 'route_pool' ? 'route_pool' : undefined,
    endpoint:
      pickString(extra.endpoint)
      ?? pickString(extra.baseUrl)
      ?? pickString(extra.base_url)
      ?? pickString(credentials.base_url)
      ?? pickString(credentials.baseURL)
      ?? pickString(credentials.url)
      ?? catalogRowEndpoint(credentials),
  };
}

function pickString(v: unknown): string | undefined {
  return typeof v === 'string' && v.trim() ? v.trim() : undefined;
}

function catalogRowEndpoint(credentials: Record<string, unknown>): string | undefined {
  const row = credentials.catalog_row;
  if (!row || typeof row !== 'object' || Array.isArray(row)) return undefined;
  const options = (row as Record<string, unknown>).options;
  if (!options || typeof options !== 'object' || Array.isArray(options)) return undefined;
  return pickString((options as Record<string, unknown>).baseURL)
    ?? pickString((options as Record<string, unknown>).base_url);
}

/** `**XXXX` from an already-stored mask. Does not invent a tail. */
export function secretTailFromMaskedPreview(value: string | undefined | null): string | undefined {
  if (!value) return undefined;
  const stripped = value
    .trim()
    .replace(/\s*（API Key）\s*$/i, '')
    .replace(/\s*\(API Key\)\s*$/i, '')
    .trim();
  if (!stripped) return undefined;
  const stars = stripped.match(/^\*{2}([A-Za-z0-9]{4})$/);
  if (stars?.[1]) return `**${stars[1]}`;
  const dotted = stripped.match(/[•*….]{2,}([A-Za-z0-9]{4})$/);
  if (dotted?.[1]) return `**${dotted[1]}`;
  return undefined;
}

function hasNonEmptyField(value: unknown, names: string[]): boolean {
  if (!value || typeof value !== 'object') return false;
  if (Array.isArray(value)) return value.some((item) => hasNonEmptyField(item, names));
  const object = value as Record<string, unknown>;
  if (names.some((name) => pickString(object[name]))) return true;
  return Object.values(object).some((child) => hasNonEmptyField(child, names));
}

function looksLikeEmail(s: string): boolean {
  return s.includes('@') && !s.includes(' ');
}

function shortId(raw: string): string {
  const t = raw.trim();
  if (t.length > 12 && t.includes('-')) {
    const head = t.split('-')[0] ?? t;
    if (head.length >= 8) return `${head}…`;
  }
  return t.length > 16 ? `${t.slice(0, 12)}…` : t;
}

function inferProviderFromBody(body: unknown): string | undefined {
  if (!body || typeof body !== 'object' || Array.isArray(body)) return undefined;
  const keys = Object.keys(body as Record<string, unknown>);
  if (keys.length === 1) return keys[0];
  const preferred = [
    'anthropic',
    'openai-codex',
    'xai',
    'github-copilot',
    'openrouter',
    'kimi-coding',
  ];
  return preferred.find((k) => keys.includes(k)) ?? keys[0];
}

/** Upgrade legacy placeholder titles when we have better identity. */
function improveGenericOAuthLabel(
  raw: string,
  bits: {
    provider?: string;
    identityLabel?: string;
    email?: string;
    subjectId?: string;
    agentId?: string;
  },
): string | undefined {
  const t = raw.trim();
  const weak =
    /\(oauth\)$/i.test(t) ||
    /-oauth$/i.test(t) ||
    / · oauth$/i.test(t) ||
    / oauth$/i.test(t) ||
    t === 'pi-auth' ||
    t === 'codex-oauth' ||
    t === 'grok-oauth' ||
    t === 'kimi-oauth' ||
    t === 'claude-oauth' ||
    (t.startsWith('pi:') && t === bits.identityLabel);
  if (!weak) return undefined;

  const email = bits.email && looksLikeEmail(bits.email) ? bits.email : undefined;
  const niceIdentity =
    bits.identityLabel && looksLikeEmail(bits.identityLabel)
      ? bits.identityLabel
      : bits.identityLabel && !looksLikeUuid(bits.identityLabel)
        ? bits.identityLabel
        : undefined;
  const id = email ?? niceIdentity ?? (bits.subjectId ? shortId(bits.subjectId) : undefined);

  if (bits.agentId === 'pi' || t.startsWith('pi:')) {
    if (bits.provider && id) return `pi:${bits.provider} · ${id}`;
    if (id) return id;
    if (bits.provider) return `pi:${bits.provider}`;
  }
  return id;
}

/** `**xxxx (API Key)` → `**xxxx (grok)` when the catalog provider name is known. */
function improveGenericApiKeyLabel(
  raw: string,
  provider: string | undefined,
  agentId?: string,
): string | undefined {
  const name = provider?.trim();
  if (!name || name === agentId) return undefined;
  const t = raw.trim();
  if (!/\(\s*API Key\s*\)$/i.test(t)) return undefined;
  if (t.toLowerCase().includes(name.toLowerCase())) return undefined;
  return t.replace(/\(\s*API Key\s*\)$/i, `(${name})`);
}

function looksLikeUuid(s: string): boolean {
  const t = s.trim();
  return /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(t);
}

/** Build a short non-secret summary for the account detail panel. */
function buildCredentialSummary(
  credentials: Record<string, unknown>,
  bits: { format?: string; envKey?: string; source?: string; provider?: string },
): string | undefined {
  const parts: string[] = [];
  if (bits.provider) parts.push(`provider=${bits.provider}`);
  if (bits.format) parts.push(`format=${bits.format}`);
  if (bits.envKey) parts.push(`env=${bits.envKey}`);
  if (bits.source) parts.push(`source=${bits.source}`);
  // Non-secret structural keys that help verify shape after redaction.
  if (credentials.body !== undefined) {
    const body = credentials.body;
    if (body && typeof body === 'object' && !Array.isArray(body)) {
      const keys = Object.keys(body as Record<string, unknown>);
      if (keys.length) parts.push(`bodyKeys=${keys.slice(0, 6).join(',')}`);
    } else {
      parts.push('body=***');
    }
  }
  if (typeof credentials.api_key === 'string') {
    parts.push(credentials.api_key === '***' ? 'api_key=***' : 'api_key=set');
  }
  if (credentials.access_token === '***' || credentials.refresh_token === '***') {
    parts.push('oauth=set');
  }
  return parts.length ? parts.join(' · ') : undefined;
}

export type AccountAuthView = {
  account: Account;
  savedAuth: AuthHealth | 'unset';
  /** `extra.authHealth` before overlaying a live probe. */
  liveAuthFromExtra: AuthHealth | 'unset';
};

function extraRecord(core: CoreAccount): Record<string, unknown> {
  return core.extra ?? {};
}

/** Pool-row health only. Does not read collapsed `Account.authHealth`. */
export function savedAuthFromCore(core: CoreAccount): AuthHealth | 'unset' {
  return normalizeAuthHealth(core.health ?? extraRecord(core).health) ?? 'unset';
}

export function liveAuthFromCore(
  core: CoreAccount,
  probe?: LiveAuthProbe,
): AuthHealth | 'unset' {
  const fromProbe = normalizeAuthHealth(probe?.health);
  if (fromProbe) return fromProbe;
  return normalizeAuthHealth(extraRecord(core).authHealth) ?? 'unset';
}

export function mapCoreAccountView(core: CoreAccount): AccountAuthView {
  return {
    account: mapCoreAccount(core),
    savedAuth: savedAuthFromCore(core),
    liveAuthFromExtra: normalizeAuthHealth(extraRecord(core).authHealth) ?? 'unset',
  };
}

function isAccountAuthView(row: AccountAuthView | Account): row is AccountAuthView {
  return 'savedAuth' in row && 'liveAuthFromExtra' in row && 'account' in row;
}

/** Bare Account (mock / uncarried pool row) is always unset. */
export function savedAuthOf(row: AccountAuthView | Account): AuthHealth | 'unset' {
  if (isAccountAuthView(row)) return row.savedAuth;
  return 'unset';
}

export function liveAuthOf(
  row: AccountAuthView | Account,
  probe?: LiveAuthProbe,
): AuthHealth | 'unset' {
  const fromProbe = normalizeAuthHealth(probe?.health);
  if (fromProbe) return fromProbe;
  const account = isAccountAuthView(row) ? row.account : row;
  const fromLive = normalizeAuthHealth(account.liveAuthHealth);
  if (fromLive) return fromLive;
  if (isAccountAuthView(row)) return row.liveAuthFromExtra;
  return 'unset';
}

/** Mock / uncarried pool rows: no CoreAccount provenance. */
export function wrapBareAccount(account: Account): AccountAuthView {
  return {
    account,
    savedAuth: 'unset',
    liveAuthFromExtra: normalizeAuthHealth(account.liveAuthHealth) ?? 'unset',
  };
}

export function unwrapAccount(row: AccountAuthView | Account): Account {
  return isAccountAuthView(row) ? row.account : row;
}

export function unwrapAccounts(rows: readonly (AccountAuthView | Account)[]): Account[] {
  return rows.map(unwrapAccount);
}

function remainingSecFromExpiresAt(raw: unknown): number | undefined {
  if (raw == null) return undefined;
  let expMs: number | undefined;
  if (typeof raw === 'number' && Number.isFinite(raw)) {
    // seconds vs millis heuristic (aligned with core is_claude_token_expired)
    expMs = raw > 1_000_000_000_000 ? raw : raw * 1000;
  } else if (typeof raw === 'string' && raw.trim()) {
    const n = Number(raw);
    if (Number.isFinite(n) && n > 0) {
      expMs = n > 1_000_000_000_000 ? n : n * 1000;
    } else {
      const t = Date.parse(raw);
      if (!Number.isNaN(t)) expMs = t;
    }
  }
  if (expMs === undefined) return undefined;
  const rem = Math.floor((expMs - Date.now()) / 1000);
  return rem;
}

/** Absolute remaining seconds → "2h05m 后重置" / "45m 后重置" / "即将重置". */
function formatQuotaResetIn(sec: number | undefined): string | undefined {
  if (sec === undefined) return undefined;
  if (sec <= 0) return '即将重置';
  const h = Math.floor(sec / 3600);
  const m = Math.floor((sec % 3600) / 60);
  if (h >= 24) {
    const d = Math.floor(h / 24);
    const rh = h % 24;
    return `${d}d${rh}h 后重置`;
  }
  if (h === 0) return `${m}m 后重置`;
  return `${h}h${String(m).padStart(2, '0')}m 后重置`;
}

/** 按身份分组账号（同人多授权并列；组内 current 优先，再按更新时间新→旧） */
export function groupAccountsByIdentity(
  accounts: Account[],
): Array<{ identity: string; accounts: Account[] }> {
  const order: string[] = [];
  const map = new Map<string, Account[]>();
  for (const acc of accounts) {
    const key = (acc.identityLabel ?? acc.email ?? acc.label ?? acc.id).trim() || acc.id;
    if (!map.has(key)) {
      order.push(key);
      map.set(key, []);
    }
    map.get(key)!.push(acc);
  }
  return order.map((identity) => {
    const list = map.get(identity) ?? [];
    list.sort((a, b) => {
      if (a.isCurrent !== b.isCurrent) return a.isCurrent ? -1 : 1;
      const ta = a.updatedAt ?? a.createdAt ?? '';
      const tb = b.updatedAt ?? b.createdAt ?? '';
      return tb.localeCompare(ta);
    });
    return { identity, accounts: list };
  });
}
