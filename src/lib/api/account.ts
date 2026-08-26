/**
 * Account API façade — delegates to app runtime backend.
 * Pages may keep importing from here during progressive migration.
 */
import {
  getBackend,
  markConnectionCurrent,
  refreshRuntimeReadModels,
} from '@/app/runtime';
import type {
  AuthState,
  DeviceOAuthPollInfo,
  DeviceOAuthStartInfo,
  LiveAuthProbe,
  OAuthLoginOption,
  OAuthStartInfo,
  OAuthWaitInfo,
} from '@/lib/backend/contracts/ports';
import {
  clearLiveAuthProbeCache as clearProbeCache,
  probeLiveAuthWithPort,
} from '@/lib/backend/contracts/live-auth-probe-cache';
import type { Account, AgentId } from '@/lib/types';
import { OAUTH_WAIT_TIMEOUT_SECS } from '@/lib/backend/contracts/oauth-constants';

export type {
  CoreAccount,
  CoreAccountSwitchResult,
} from '@/lib/backend/contracts/account-map';
export { mapCoreAccount } from '@/lib/backend/contracts/account-map';
export type {
  DeviceOAuthPollInfo,
  DeviceOAuthStartInfo,
  AuthState,
  LiveAuthProbe,
  OAuthLoginOption,
  OAuthStartInfo,
  OAuthWaitInfo,
};
export { OAUTH_WAIT_TIMEOUT_SECS };

export async function listAccounts(agentId?: AgentId): Promise<Account[]> {
  return getBackend().account.listAccounts(agentId);
}

/** Re-read live files into the pool, then refresh the shared store. */
export async function reconcileAccountPool(agentId?: AgentId): Promise<void> {
  const backend = getBackend();
  if (!backend.account.reconcileAccounts) return;
  await backend.account.reconcileAccounts(agentId);
  await refreshRuntimeReadModels(backend, { models: ['connectionPool'] });
}

/**
 * Probe cache invalidation is followed by a background shared-status refresh.
 * Pages read the AgentStatus store, so this keeps Dashboard/Connections in
 * sync without every mutation handler issuing its own live probe.
 */
function authStateChanged(agentId: AgentId): void {
  void refreshRuntimeReadModels(getBackend(), {
    agentId,
    clearProbe: true,
  });
}

/** Reconcile an externally rotated/current login through the shared store. */
export function refreshLiveAuthState(agentId: AgentId): void {
  authStateChanged(agentId);
}

export async function probeLiveAuth(
  agentId: AgentId,
  options: { force?: boolean } = {},
): Promise<LiveAuthProbe> {
  return probeLiveAuthWithPort(getBackend().account, agentId, options);
}

export { clearProbeCache as clearLiveAuthProbeCache };

export async function switchAccount(agentId: AgentId, accountId: string): Promise<void> {
  await getBackend().account.switchAccount(agentId, accountId);
  markConnectionCurrent(agentId, 'account', accountId);
  authStateChanged(agentId);
}

export async function undoSwitchAccount(agentId: AgentId): Promise<boolean> {
  const undone = await getBackend().account.undoSwitchAccount(agentId);
  if (undone) authStateChanged(agentId);
  return undone;
}

export async function addApiKeyAccount(
  agentId: AgentId,
  key: string,
  label?: string | null,
  envKey?: string | null,
  productMarker?: string | null,
): Promise<Account> {
  const account = await getBackend().account.addApiKeyAccount(
    agentId,
    key,
    label,
    envKey,
    productMarker,
  );
  authStateChanged(agentId);
  return account;
}

export async function updateApiKeyAccount(
  agentId: AgentId,
  accountId: string,
  opts: { label?: string | null; key?: string | null },
): Promise<Account> {
  const account = await getBackend().account.updateApiKeyAccount(agentId, accountId, opts);
  authStateChanged(agentId);
  return account;
}

export async function importCurrentLogin(agentId: AgentId): Promise<Account> {
  const account = await getBackend().account.importCurrentLogin(agentId);
  authStateChanged(agentId);
  return account;
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
  timeoutSecs = OAUTH_WAIT_TIMEOUT_SECS,
): Promise<OAuthWaitInfo> {
  return getBackend().account.waitOAuth(state, timeoutSecs);
}

export async function finishOAuth(state: string): Promise<Account> {
  const account = await getBackend().account.finishOAuth(state);
  authStateChanged(account.agentId);
  return account;
}

export async function cancelOAuth(state: string): Promise<void> {
  await getBackend().account.cancelOAuth(state);
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
  const account = await getBackend().account.finishDeviceOAuth(state);
  authStateChanged(account.agentId);
  return account;
}

export async function completeOAuth(
  agentId: AgentId,
  providerKey?: string | null,
): Promise<Account> {
  const account = await getBackend().account.completeOAuth(agentId, providerKey);
  authStateChanged(agentId);
  return account;
}

export async function deleteAccount(agentId: AgentId, accountId: string): Promise<void> {
  await getBackend().account.deleteAccount(agentId, accountId);
  authStateChanged(agentId);
}

export async function refreshToken(agentId: AgentId, accountId: string): Promise<void> {
  await getBackend().account.refreshToken(agentId, accountId);
  authStateChanged(agentId);
}

/** Force-refresh upstream 5h/7d quota for OAuth (no-op when unsupported). */
export async function refreshQuota(
  agentId: AgentId,
  accountId: string,
): Promise<Account | undefined> {
  const port = getBackend().account;
  if (!port.refreshQuota) return undefined;
  const account = await port.refreshQuota(agentId, accountId);
  void refreshRuntimeReadModels(getBackend(), { models: ['connectionPool'] });
  return account;
}
