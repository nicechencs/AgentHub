/**
 * Sub2API façade — HTTP helpers. Native password login is primary;
 * child webview open-login remains available on the settings port but is unused by UI.
 */
import { getBackend } from '@/app/runtime';
import {
  clearSub2ApiSession,
  createApiKey,
  deleteApiKey,
  fetchCurrentUser,
  listAvailableGroups,
  updateApiKey,
  fetchPublicSettings,
  isTotp2FARequired,
  listApiKeys,
  loadSub2ApiSession,
  loginWith2FA,
  loginWithPassword,
  logoutRemote,
  refreshAuthTokens,
  saveSub2ApiSession,
  sessionFromTokens,
  sessionNeedsRefresh,
  sub2apiGatewayBaseUrl,
  sub2apiLoginUrl,
  syncSub2ApiKeyToConnections,
  SUB2API_DEFAULT_SITE_URL,
  buildLoginBody,
  clearAllRememberedAccounts,
  clearAllRememberedAccountsAsync,
  clearAllRememberedPasswords,
  clearAllRememberedPasswordsAsync,
  deleteRememberedAccount,
  deleteRememberedAccountAsync,
  deleteRememberedSite,
  getLastUsedRememberedAccount,
  hydrateRememberedPasswordVault as hydrateRememberedPasswordVaultInner,
  setRememberedVaultTransport,
  isSub2ApiRememberEnabled,
  listRememberedAccounts,
  listRememberedSites,
  loadRememberedCredentials,
  saveRememberedAccount,
  saveRememberedAccountAsync,
  saveRememberedSite,
  seedRememberedSitesIfUnset,
  setSub2ApiRememberEnabled,
  type Sub2ApiAuthTokens,
  type Sub2ApiCaptchaProof,
  type Sub2ApiGroup,
  type Sub2ApiKey,
  type Sub2ApiKeyPatch,
  type Sub2ApiLoginResult,
  type Sub2ApiPublicSettings,
  type Sub2ApiRememberedAccountMeta,
  type Sub2ApiSession,
  type Sub2ApiUser,
} from '@/lib/sub2api';

export type {
  Sub2ApiGroup,
  Sub2ApiKey,
  Sub2ApiKeyPatch,
  Sub2ApiPublicSettings,
  Sub2ApiSession,
  Sub2ApiUser,
  Sub2ApiCaptchaProof,
  Sub2ApiRememberedAccountMeta,
};
export {
  clearSub2ApiSession,
  loadSub2ApiSession,
  saveSub2ApiSession,
  sessionFromTokens,
  sessionNeedsRefresh,
  sub2apiLoginUrl,
  sub2apiGatewayBaseUrl,
  syncSub2ApiKeyToConnections,
  SUB2API_DEFAULT_SITE_URL,
  isTotp2FARequired,
  buildLoginBody,
  clearAllRememberedAccounts,
  clearAllRememberedAccountsAsync,
  clearAllRememberedPasswords,
  clearAllRememberedPasswordsAsync,
  deleteRememberedAccount,
  deleteRememberedAccountAsync,
  deleteRememberedSite,
  getLastUsedRememberedAccount,
  isSub2ApiRememberEnabled,
  listRememberedAccounts,
  listRememberedSites,
  loadRememberedCredentials,
  saveRememberedAccount,
  saveRememberedAccountAsync,
  saveRememberedSite,
  seedRememberedSitesIfUnset,
  setSub2ApiRememberEnabled,
};

/** @deprecated Native login is primary; kept for optional/legacy callers. */
export async function openSub2ApiLoginWindow(loginUrl: string): Promise<{
  accessToken: string;
  refreshToken?: string;
  expiresAt?: number;
}> {
  return getBackend().settings.openSub2ApiLoginWindow(loginUrl);
}

/** @deprecated Native login is primary; kept for optional/legacy callers. */
export async function closeSub2ApiLoginWindow(): Promise<void> {
  await getBackend().settings.closeSub2ApiLoginWindow();
}

/** Hydrate password vault via settings port (SQLite on desktop; memory in mock). */
export async function hydrateRememberedPasswordVault(): Promise<void> {
  const settings = getBackend().settings;
  setRememberedVaultTransport({
    get: () => settings.getSub2ApiRememberedVault(),
    set: (json) => settings.setSub2ApiRememberedVault(json),
  });
  await hydrateRememberedPasswordVaultInner();
}

export async function probeSub2ApiPublicSettings(siteUrl: string): Promise<Sub2ApiPublicSettings> {
  return fetchPublicSettings({ siteUrl });
}

export async function loadSub2ApiUser(session: Sub2ApiSession): Promise<Sub2ApiUser> {
  return fetchCurrentUser({ siteUrl: session.siteUrl, accessToken: session.accessToken });
}

export async function loadSub2ApiKeys(session: Sub2ApiSession): Promise<Sub2ApiKey[]> {
  const list = await listApiKeys({ siteUrl: session.siteUrl, accessToken: session.accessToken });
  return list.items ?? [];
}

export async function loadSub2ApiGroups(session: Sub2ApiSession): Promise<Sub2ApiGroup[]> {
  return listAvailableGroups({ siteUrl: session.siteUrl, accessToken: session.accessToken });
}

export async function createSub2ApiKey(
  session: Sub2ApiSession,
  name: string,
  groupId?: number | null,
): Promise<Sub2ApiKey> {
  return createApiKey(
    { siteUrl: session.siteUrl, accessToken: session.accessToken },
    name,
    groupId,
  );
}

export async function updateSub2ApiKey(
  session: Sub2ApiSession,
  id: number,
  patch: Sub2ApiKeyPatch,
): Promise<Sub2ApiKey> {
  return updateApiKey(
    { siteUrl: session.siteUrl, accessToken: session.accessToken },
    id,
    patch,
  );
}

export async function updateSub2ApiKeyGroup(
  session: Sub2ApiSession,
  id: number,
  groupId: number | null,
): Promise<Sub2ApiKey> {
  return updateSub2ApiKey(session, id, { group_id: groupId });
}

export async function deleteSub2ApiKey(session: Sub2ApiSession, id: number): Promise<void> {
  await deleteApiKey({ siteUrl: session.siteUrl, accessToken: session.accessToken }, id);
}

export async function refreshSub2ApiSession(session: Sub2ApiSession): Promise<Sub2ApiSession> {
  if (!session.refreshToken) return session;
  const tokens = await refreshAuthTokens(
    { siteUrl: session.siteUrl, accessToken: session.accessToken },
    session.refreshToken,
  );
  const next = sessionFromTokens({
    siteUrl: session.siteUrl,
    gatewayBaseUrl: session.gatewayBaseUrl,
    accessToken: tokens.access_token,
    refreshToken: tokens.refresh_token ?? session.refreshToken,
    expiresIn: tokens.expires_in,
    user: session.user,
  });
  saveSub2ApiSession(next);
  return next;
}

/**
 * Quiet boot helper: keep a still-valid session, silently refresh when near
 * expiry, or return null (caller clears UI to prefilled login) on failure.
 * Does not clear remembered accounts.
 */
export async function ensureSub2ApiSessionFresh(
  session: Sub2ApiSession | null,
): Promise<Sub2ApiSession | null> {
  if (!session?.accessToken?.trim()) return null;
  if (!sessionNeedsRefresh(session)) {
    // Still probe /auth/me lightly only when no expiry — keep token as-is.
    return session;
  }
  try {
    return await refreshSub2ApiSession(session);
  } catch {
    clearSub2ApiSession();
    return null;
  }
}

export async function logoutSub2Api(session: Sub2ApiSession | null): Promise<void> {
  if (session) {
    await logoutRemote(
      { siteUrl: session.siteUrl, accessToken: session.accessToken },
      session.refreshToken,
    );
  }
  clearSub2ApiSession();
}

export async function establishSessionFromTokens(input: {
  siteUrl: string;
  accessToken: string;
  refreshToken?: string;
  expiresAt?: number;
  expiresIn?: number;
  user?: Sub2ApiUser | null;
}): Promise<Sub2ApiSession> {
  let gatewayBaseUrl = sub2apiGatewayBaseUrl(input.siteUrl);
  try {
    const pub = await fetchPublicSettings({ siteUrl: input.siteUrl });
    gatewayBaseUrl = sub2apiGatewayBaseUrl(input.siteUrl, pub);
  } catch {
    /* keep root */
  }
  const draft = sessionFromTokens({
    siteUrl: input.siteUrl,
    gatewayBaseUrl,
    accessToken: input.accessToken,
    refreshToken: input.refreshToken,
    expiresAt: input.expiresAt,
    expiresIn: input.expiresIn,
    user: input.user,
  });
  const user =
    input.user
    ?? (await fetchCurrentUser({
      siteUrl: draft.siteUrl,
      accessToken: draft.accessToken,
    }));
  const next = { ...draft, user };
  saveSub2ApiSession(next);
  return next;
}

export async function nativeSub2ApiLogin(input: {
  siteUrl: string;
  email: string;
  password: string;
  captcha?: Sub2ApiCaptchaProof | null;
}): Promise<Sub2ApiLoginResult> {
  const body = buildLoginBody(input.email, input.password, input.captcha);
  return loginWithPassword(input.siteUrl, body);
}

export async function nativeSub2ApiLogin2FA(input: {
  siteUrl: string;
  tempToken: string;
  totpCode: string;
}): Promise<Sub2ApiAuthTokens> {
  return loginWith2FA(input.siteUrl, {
    temp_token: input.tempToken,
    totp_code: input.totpCode,
  });
}
