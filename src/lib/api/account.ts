/**
 * Account API façade — delegates to app runtime backend.
 * Pages may keep importing from here during progressive migration.
 */
import { getBackend } from '@/app/runtime';
import type {
  DeviceOAuthPollInfo,
  DeviceOAuthStartInfo,
  LiveAuthProbe,
  OAuthLoginOption,
  OAuthStartInfo,
  OAuthWaitInfo,
} from '@/lib/backend/contracts/ports';
import type { Account, AgentId } from '@/lib/types';

export type {
  CoreAccount,
  CoreAccountSwitchResult,
} from '@/lib/backend/contracts/account-map';
export { mapCoreAccount } from '@/lib/backend/contracts/account-map';
export type {
  DeviceOAuthPollInfo,
  DeviceOAuthStartInfo,
  LiveAuthProbe,
  OAuthLoginOption,
  OAuthStartInfo,
  OAuthWaitInfo,
};

export async function listAccounts(agentId?: AgentId): Promise<Account[]> {
  return getBackend().account.listAccounts(agentId);
}

/** Read-only probe of the live auth kind; never returns credential material. */
export async function probeLiveAuth(agentId: AgentId): Promise<LiveAuthProbe> {
  return getBackend().account.probeLiveAuth(agentId);
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

export async function listOAuthOptions(agentId: AgentId): Promise<OAuthLoginOption[]> {
  return getBackend().account.listOAuthOptions(agentId);
}

export async function startOAuth(
  agentId: AgentId,
  openBrowser = true,
  providerKey?: string | null,
): Promise<OAuthStartInfo> {
  return getBackend().account.startOAuth(agentId, openBrowser, providerKey);
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

export async function startDeviceOAuth(
  agentId: AgentId,
  providerKey: string,
): Promise<DeviceOAuthStartInfo> {
  return getBackend().account.startDeviceOAuth(agentId, providerKey);
}

export async function pollDeviceOAuth(state: string): Promise<DeviceOAuthPollInfo> {
  return getBackend().account.pollDeviceOAuth(state);
}

export async function finishDeviceOAuth(state: string): Promise<Account> {
  return getBackend().account.finishDeviceOAuth(state);
}

export async function completeOAuth(
  agentId: AgentId,
  providerKey?: string | null,
): Promise<Account> {
  return getBackend().account.completeOAuth(agentId, providerKey);
}

export async function deleteAccount(agentId: AgentId, accountId: string): Promise<void> {
  return getBackend().account.deleteAccount(agentId, accountId);
}

export async function refreshToken(agentId: AgentId, accountId: string): Promise<void> {
  return getBackend().account.refreshToken(agentId, accountId);
}

/** Force-refresh upstream 5h/7d quota for OAuth (no-op when unsupported). */
export async function refreshQuota(
  agentId: AgentId,
  accountId: string,
): Promise<Account | undefined> {
  const port = getBackend().account;
  if (!port.refreshQuota) return undefined;
  return port.refreshQuota(agentId, accountId);
}
