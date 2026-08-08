/**
 * Account API façade — delegates to app runtime backend.
 * Pages may keep importing from here during progressive migration.
 */
import { getBackend } from '@/app/runtime';
import type { OAuthStartInfo, OAuthWaitInfo } from '@/lib/backend/contracts/ports';
import type { Account, AgentId } from '@/lib/types';

export type {
  CoreAccount,
  CoreAccountSwitchResult,
} from '@/lib/backend/contracts/account-map';
export { mapCoreAccount } from '@/lib/backend/contracts/account-map';
export type { OAuthStartInfo, OAuthWaitInfo };

export async function listAccounts(agentId?: AgentId): Promise<Account[]> {
  return getBackend().account.listAccounts(agentId);
}

export async function switchAccount(agentId: AgentId, accountId: string): Promise<void> {
  return getBackend().account.switchAccount(agentId, accountId);
}

export async function undoSwitchAccount(agentId: AgentId): Promise<boolean> {
  return getBackend().account.undoSwitchAccount(agentId);
}

export async function addApiKeyAccount(
  agentId: AgentId,
  key: string,
  label?: string | null,
  envKey?: string | null,
): Promise<Account> {
  return getBackend().account.addApiKeyAccount(agentId, key, label, envKey);
}

export async function updateApiKeyAccount(
  agentId: AgentId,
  accountId: string,
  opts: { label?: string | null; key?: string | null },
): Promise<Account> {
  return getBackend().account.updateApiKeyAccount(agentId, accountId, opts);
}

export async function importCurrentLogin(agentId: AgentId): Promise<Account> {
  return getBackend().account.importCurrentLogin(agentId);
}

export async function oauthSupported(agentId: AgentId): Promise<boolean> {
  return getBackend().account.oauthSupported(agentId);
}

export async function startOAuth(
  agentId: AgentId,
  openBrowser = true,
): Promise<OAuthStartInfo> {
  return getBackend().account.startOAuth(agentId, openBrowser);
}

export async function waitOAuth(
  state: string,
  timeoutSecs = 120,
): Promise<OAuthWaitInfo> {
  return getBackend().account.waitOAuth(state, timeoutSecs);
}

export async function finishOAuth(state: string): Promise<Account> {
  return getBackend().account.finishOAuth(state);
}

export async function completeOAuth(agentId: AgentId): Promise<Account> {
  return getBackend().account.completeOAuth(agentId);
}

export async function deleteAccount(agentId: AgentId, accountId: string): Promise<void> {
  return getBackend().account.deleteAccount(agentId, accountId);
}

export async function refreshToken(agentId: AgentId, accountId: string): Promise<void> {
  return getBackend().account.refreshToken(agentId, accountId);
}
