/**
 * Sub2API façade — HTTP helpers. Native password login is primary;
 * child webview open-login remains available on the settings port but is unused by UI.
 */
import { getBackend } from '@/app/runtime';
import {
  clearSub2ApiSession,
  createApiKey,
  fetchCurrentUser,
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
  sub2apiGatewayBaseUrl,
  sub2apiLoginUrl,
  syncSub2ApiKeyToConnections,
  SUB2API_DEFAULT_SITE_URL,
  buildLoginBody,
  type Sub2ApiAuthTokens,
  type Sub2ApiCaptchaProof,
  type Sub2ApiKey,
  type Sub2ApiLoginResult,
  type Sub2ApiPublicSettings,
  type Sub2ApiSession,
  type Sub2ApiUser,
} from '@/lib/sub2api';

export type { Sub2ApiKey, Sub2ApiPublicSettings, Sub2ApiSession, Sub2ApiUser, Sub2ApiCaptchaProof };
export {
  clearSub2ApiSession,
  loadSub2ApiSession,
  saveSub2ApiSession,
  sessionFromTokens,
  sub2apiLoginUrl,
  sub2apiGatewayBaseUrl,
  syncSub2ApiKeyToConnections,
  SUB2API_DEFAULT_SITE_URL,
  isTotp2FARequired,
  buildLoginBody,
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

export async function createSub2ApiKey(session: Sub2ApiSession, name: string): Promise<Sub2ApiKey> {
  return createApiKey({ siteUrl: session.siteUrl, accessToken: session.accessToken }, name);
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
