import type { Account, AgentId } from '@/lib/types';

export type AccountActionKind = 'sync-current-login' | 'refresh-credentials';

export interface AccountAction {
  kind: AccountActionKind;
  label: string;
}
const PI_REFRESH_PROVIDERS = new Set(['anthropic', 'openai', 'openai-codex', 'xai']);

/**
 * Central account action policy. Components must not infer provider-specific
 * refresh behavior themselves.
 */
export function accountActionPolicy(account: Pick<
  Account,
  'agentId' | 'kind' | 'provider' | 'refreshable'
>): AccountAction | undefined {
  if (account.kind !== 'oauth') return undefined;
  if (account.agentId === 'grok') {
    return { kind: 'sync-current-login', label: '同步当前登录' };
  }
  // These CLIs own their OAuth lifecycle; AgentHub only exposes import/check.
  if (account.agentId === 'kimi' || account.agentId === 'codex' || account.agentId === 'claude') {
    return undefined;
  }
  if (
    account.agentId === 'pi' &&
    account.refreshable === true &&
    PI_REFRESH_PROVIDERS.has(account.provider?.trim().toLowerCase() ?? '')
  ) {
    return { kind: 'refresh-credentials', label: '刷新凭据' };
  }
  return undefined;
}

export const getAccountActionPolicy = accountActionPolicy;

/** Testable provider matrix without exposing mutable policy internals. */
export function canRefreshAccountCredentials(
  agentId: AgentId,
  provider?: string,
  refreshable?: boolean,
): boolean {
  return (
    accountActionPolicy({ agentId, kind: 'oauth', provider, refreshable })?.kind ===
    'refresh-credentials'
  );
}
