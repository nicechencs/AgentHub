import type { MessageKey, TranslateFn } from '@/lib/i18n';
import type { Account, AgentStatus, AuthStatus } from '@/lib/types';

/** Backend/frontend wire health states. These describe what is known, not token
 * validity inferred from an expiry timestamp. */
export type AuthHealth =
  | 'verified'
  | 'renewable'
  | 'configured'
  | 'needs_login'
  | 'unknown'
  | 'missing';

export const AUTH_HEALTH_LABEL: Record<AuthHealth, string> = {
  verified: '已验证',
  renewable: '可续期',
  configured: '已配置',
  needs_login: '需要重新登录',
  unknown: '状态未知',
  missing: '未登录',
};

export interface AuthDisplay {
  health: AuthHealth;
  label: string;
  /** Legacy four-state projection used by StatusDot and old callers. */
  legacyStatus: AuthStatus;
}

const AUTH_HEALTH_VALUES = new Set<AuthHealth>([
  'verified',
  'renewable',
  'configured',
  'needs_login',
  'unknown',
  'missing',
]);

export function normalizeAuthHealth(value: unknown): AuthHealth | undefined {
  if (typeof value !== 'string') return undefined;
  const normalized = value.trim().toLowerCase().replace(/[- ]/g, '_');
  return AUTH_HEALTH_VALUES.has(normalized as AuthHealth)
    ? (normalized as AuthHealth)
    : undefined;
}

const AUTH_HEALTH_KEY: Record<AuthHealth, MessageKey> = {
  verified: 'kind.health.verified',
  renewable: 'kind.health.renewable',
  configured: 'kind.health.configured',
  needs_login: 'kind.health.needs_login',
  unknown: 'kind.health.unknown',
  missing: 'kind.health.missing',
};

export function authHealthLabel(health: AuthHealth, t?: TranslateFn): string {
  return t ? t(AUTH_HEALTH_KEY[health]) : AUTH_HEALTH_LABEL[health];
}

/** Map new semantic states to the old status-dot colors without losing meaning. */
export function authHealthToLegacyStatus(
  health: AuthHealth,
  account?: Pick<Account, 'tokenRemainingSec' | 'refreshable'>,
): AuthStatus {
  if (health === 'missing') return 'none';
  if (health === 'needs_login') return 'expired';
  if (health === 'renewable' || health === 'configured' || health === 'verified') {
    return 'valid';
  }
  // Unknown means precisely that: a legacy expiry timestamp is not proof that
  // credentials are invalid. Keep a short/stale timestamp at warning level
  // rather than rendering it as the red "expired" state.
  if (health === 'unknown' && account?.tokenRemainingSec !== undefined) {
    if (account.tokenRemainingSec <= 3 * 3600 && !account.refreshable) return 'expiring';
  }
  return 'valid';
}

function inferredAccountHealth(account: Pick<
  Account,
  'kind' | 'tokenValid' | 'refreshable' | 'tokenRemainingSec' | 'authHealth' | 'liveAuthHealth'
>): AuthHealth {
  if (account.liveAuthHealth) return account.liveAuthHealth;
  if (account.authHealth) return account.authHealth;
  if (account.kind === 'apikey') {
    // API keys are only configured until a backend explicitly verifies them.
    return account.tokenValid ? 'configured' : 'needs_login';
  }
  if (account.refreshable) return 'renewable';
  if (!account.tokenValid || (account.tokenRemainingSec ?? 1) <= 0) return 'needs_login';
  // A legacy boolean/token expiry is not proof of a successful upstream check.
  return 'unknown';
}

/** Single account-to-display mapping used by Accounts, Connections and API façade. */
export function authDisplayForAccount(account: Account, t?: TranslateFn): AuthDisplay {
  const health = inferredAccountHealth(account);
  return {
    health,
    label: authHealthLabel(health, t),
    legacyStatus: authHealthToLegacyStatus(health, account),
  };
}

/** Apply the centrally probed live state only to the account currently in use. */
export function attachLiveAgentAuth(account: Account, status: AgentStatus | undefined): Account {
  if (!account.isCurrent) return account;
  const liveAuthHealth = normalizeAuthHealth(status?.authHealth);
  if (!liveAuthHealth) return account;
  return {
    ...account,
    liveAuthHealth,
    liveAuthSource: status?.authSource ?? account.liveAuthSource,
    liveAuthRevision: status?.authRevision ?? account.liveAuthRevision,
  };
}

/** Naming aliases for callers migrating from the old authStatus helpers. */
export const authStateForAccount = authDisplayForAccount;

/** Map an AgentStatus while accepting old backend rows without health fields. */
export function authDisplayForAgentStatus(
  status: AgentStatus | undefined,
  t?: TranslateFn,
): AuthDisplay {
  if (!status || !status.installed) {
    return { health: 'missing', label: authHealthLabel('missing', t), legacyStatus: 'none' };
  }
  const explicit = normalizeAuthHealth(status.authHealth);
  if (explicit) {
    return {
      health: explicit,
      label: authHealthLabel(explicit, t),
      legacyStatus: authHealthToLegacyStatus(explicit),
    };
  }
  // Old doctor values are retained for compatibility, but renewable/unknown
  // rows are no longer treated as invalid by Dashboard.
  if (status.authStatus === 'expired') {
    return { health: 'needs_login', label: authHealthLabel('needs_login', t), legacyStatus: 'expired' };
  }
  if (status.authStatus === 'none') {
    return { health: 'missing', label: authHealthLabel('missing', t), legacyStatus: 'none' };
  }
  if (status.authLabel === 'API' || status.effectiveKind === 'api') {
    return { health: 'configured', label: authHealthLabel('configured', t), legacyStatus: 'valid' };
  }
  if (status.authStatus === 'expiring') {
    return { health: 'unknown', label: authHealthLabel('unknown', t), legacyStatus: 'expiring' };
  }
  return { health: 'unknown', label: authHealthLabel('unknown', t), legacyStatus: 'valid' };
}

export const authStateForAgentStatus = authDisplayForAgentStatus;
