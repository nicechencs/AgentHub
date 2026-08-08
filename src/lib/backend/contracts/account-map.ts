import type { Account, AccountKind, AgentId } from '@/lib/types';

export interface CoreAccount {
  id: string;
  agentId: AgentId;
  kind: AccountKind;
  label: string;
  credentials?: Record<string, unknown>;
  extra?: Record<string, unknown>;
  status: string;
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
  const email = typeof extra.email === 'string' ? extra.email : undefined;
  const identityLabel =
    typeof extra.identityLabel === 'string' && extra.identityLabel.trim()
      ? extra.identityLabel.trim()
      : email ?? a.label;
  const subscription =
    typeof extra.subscription === 'string' ? extra.subscription : undefined;
  let tokenRemainingSec =
    typeof extra.tokenRemainingSec === 'number' ? extra.tokenRemainingSec : undefined;
  // Derive remaining from expiresAt when adapter only stored absolute expiry.
  if (tokenRemainingSec === undefined) {
    tokenRemainingSec = remainingSecFromExpiresAt(extra.expiresAt);
  }
  const quota5hPct = typeof extra.quota5hPct === 'number' ? extra.quota5hPct : undefined;
  const quota7dPct = typeof extra.quota7dPct === 'number' ? extra.quota7dPct : undefined;
  const quotaResetIn = typeof extra.quotaResetIn === 'string' ? extra.quotaResetIn : undefined;
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
    typeof extra.source === 'string'
      ? extra.source
      : typeof credentials.source === 'string'
        ? credentials.source
        : undefined;
  const credentialSummary = buildCredentialSummary(credentials, {
    format: credentialFormat,
    envKey,
    source,
  });

  const tokenExpired = extra.tokenExpired === true;
  const tokenValid =
    !tokenExpired && (a.status === 'active' || a.status === '');

  return {
    id: a.id,
    agentId: a.agentId,
    kind: a.kind,
    label: a.label,
    email,
    identityLabel,
    subscription,
    isCurrent: a.isCurrent,
    tokenValid,
    status: a.status || undefined,
    tokenRemainingSec,
    quota5hPct,
    quota7dPct,
    quotaResetIn,
    lastUsedAt,
    updatedAt: a.updatedAt,
    createdAt: a.createdAt,
    credentialFormat,
    source,
    envKey,
    credentialSummary,
  };
}

/** Build a short non-secret summary for the account detail panel. */
function buildCredentialSummary(
  credentials: Record<string, unknown>,
  bits: { format?: string; envKey?: string; source?: string },
): string | undefined {
  const parts: string[] = [];
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
  return parts.length ? parts.join(' · ') : undefined;
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
