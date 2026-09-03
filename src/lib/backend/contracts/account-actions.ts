import type { Account, AgentKey } from '@/lib/types';
import { isPiRefreshProvider } from './oauth-constants';

export type AccountActionKind =
  | 'sync-current-login'
  | 'refresh-credentials'
  | 'refresh-quota';

export interface AccountAction {
  kind: AccountActionKind;
  label: string;
}

/**
 * Central account action policy. Components must not infer provider-specific
 * refresh behavior themselves.
 */
export function accountActionPolicy(account: Pick<
  Account,
  'agentId' | 'kind' | 'provider' | 'refreshable'
>): AccountAction | undefined {
  if (account.kind !== 'oauth') return undefined;
  // List-level actions decide Hub vs CLI ownership; policy stays off.
  if (
    account.agentId === 'kimi'
    || account.agentId === 'codex'
    || account.agentId === 'claude'
    || account.agentId === 'grok'
  ) {
    return undefined;
  }
  if (
    account.agentId === 'pi' &&
    account.refreshable === true &&
    isPiRefreshProvider(account.provider)
  ) {
    return { kind: 'refresh-credentials', label: '刷新登录信息' };
  }
  return undefined;
}

function isHubOwnedOauthSource(source?: string): boolean {
  return source === 'oauth_pkce' || source === 'oauth_refresh';
}

/** Every visible list-row action should follow with a 5h/7d quota probe. */
export function oauthListActionProbesQuota(kind: AccountActionKind): boolean {
  return (
    kind === 'refresh-quota'
    || kind === 'refresh-credentials'
    || kind === 'sync-current-login'
  );
}

export function oauthListAction(account: Pick<
  Account,
  'agentId' | 'kind' | 'provider' | 'refreshable' | 'source' | 'isCurrent'
>): AccountAction | undefined {
  const policy = accountActionPolicy(account);
  if (policy) return policy;
  if (account.kind !== 'oauth') return undefined;
  if (account.agentId === 'grok' || account.agentId === 'codex') {
    if (account.refreshable === true && isHubOwnedOauthSource(account.source)) {
      return { kind: 'refresh-credentials', label: '刷新' };
    }
    if (account.isCurrent) {
      return { kind: 'sync-current-login', label: '同步当前登录' };
    }
    return { kind: 'refresh-quota', label: '刷新' };
  }
  if (account.agentId === 'claude') {
    return { kind: 'refresh-quota', label: '刷新' };
  }
  return undefined;
}

export const getAccountActionPolicy = accountActionPolicy;

/** Testable provider matrix without exposing mutable policy internals. */
export function canRefreshAccountCredentials(
  agentId: AgentKey,
  provider?: string,
  refreshable?: boolean,
): boolean {
  return (
    accountActionPolicy({ agentId, kind: 'oauth', provider, refreshable })?.kind ===
    'refresh-credentials'
  );
}
