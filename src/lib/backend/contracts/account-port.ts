import type { Account, AgentKey } from '@/lib/types';
import type { AccountAuthView } from './account-map';
import { normalizeAuthHealth, type AuthHealth } from './auth-state';

/** PKCE start result from backend. */
export interface OAuthStartInfo {
  state: string;
  authorizeUrl: string;
  redirectUri: string;
  agentId: AgentKey;
  /** Pi multi-provider key when applicable. */
  providerKey?: string | null;
  browserOpened: boolean;
  /** Seconds the wait page should stay open. Matches the browser-login listener. */
  expiresInSecs?: number;
}

export interface OAuthWaitInfo {
  state: string;
  agentId: AgentKey;
  status: 'waiting' | 'callbackReceived' | 'succeeded' | 'failed';
  error?: string | null;
}

export type OAuthFlowKind = 'pkce' | 'deviceCode';

/** One selectable OAuth login target (Pi has multiple). */
export interface OAuthLoginOption {
  id: string;
  agentId: AgentKey;
  label: string;
  description: string;
  flow: OAuthFlowKind;
  authJsonKey?: string | null;
}

export interface DeviceOAuthStartInfo {
  state: string;
  agentId: AgentKey;
  providerKey: string;
  userCode: string;
  verificationUri: string;
  verificationUriComplete?: string | null;
  intervalSecs: number;
  expiresInSecs: number;
}

export interface DeviceOAuthPollInfo {
  state: string;
  status: 'pending' | 'slowDown' | 'complete' | 'completing' | 'failed' | 'expired';
  error?: string | null;
}

/** Read-only live authentication probe; never contains credential material. */
export interface AuthState {
  /** Core wire field. Older adapters may return agentId instead. */
  agent: AgentKey;
  kind?: string | null;
  summary: string;
  hasCredentials: boolean;
  /** Non-secret live-file revision used to detect external token rotation. */
  revision?: string | null;
  /** Optional backend verification result (old backends omit it). */
  health?: AuthHealth;
  /** Redacted source label (settings/auth file/live/etc.). */
  source?: string | null;
  /** Other live credential families besides `kind` (e.g. `["oauth"]` when kind is api_key). */
  alsoPresent?: string[] | null;
  /** True when live files are AgentHub's local-route write, not a user login. */
  isAdapterProjection?: boolean | null;
  /** SHA-256 of the live API key. Never the raw secret. */
  secretHash?: string | null;
}

/** `alsoPresent` marker for the same fact when `isAdapterProjection` is omitted. */
export const ADAPTER_PROJECTION_KIND = 'adapter_projection';

/** Normalized probe consumed by browser pages; keeps agentId for old callers. */
export interface LiveAuthProbe {
  agentId: AgentKey;
  kind?: string | null;
  summary: string;
  hasCredentials: boolean;
  /** Non-secret live-file revision used to detect external token rotation. */
  revision?: string | null;
  health?: AuthHealth;
  source?: string | null;
  /** Other live credential families besides `kind` (e.g. `["oauth"]` when kind is api_key). */
  alsoPresent?: string[] | null;
  isAdapterProjection?: boolean;
  /** SHA-256 of the live API key. Never the raw secret. */
  secretHash?: string | null;
}

function normalizeAlsoPresent(raw: unknown): string[] {
  if (!Array.isArray(raw)) return [];
  return raw.filter((item): item is string => typeof item === 'string');
}

export function probeIsAdapterProjection(
  probe: Pick<AuthState, 'isAdapterProjection' | 'alsoPresent'> | null | undefined,
): boolean {
  if (!probe) return false;
  if (probe.isAdapterProjection === true) return true;
  return normalizeAlsoPresent(probe.alsoPresent).some(
    (kind) => kind.trim().toLowerCase() === ADAPTER_PROJECTION_KIND,
  );
}

/** Accept both current core AuthState (`agent`) and legacy JS probe (`agentId`). */
export function normalizeAuthState(
  raw: Partial<AuthState> & { agentId?: AgentKey },
  fallbackAgentId: AgentKey,
): LiveAuthProbe {
  const agentId = raw.agentId ?? raw.agent ?? fallbackAgentId;
  const alsoPresent = normalizeAlsoPresent(raw.alsoPresent);
  return {
    agentId,
    kind: raw.kind ?? null,
    summary: typeof raw.summary === 'string' ? raw.summary : '',
    hasCredentials: raw.hasCredentials === true,
    revision: raw.revision ?? null,
    health: normalizeAuthHealth(raw.health),
    source: raw.source ?? null,
    alsoPresent,
    isAdapterProjection: probeIsAdapterProjection({
      isAdapterProjection: raw.isAdapterProjection,
      alsoPresent,
    }),
    secretHash: typeof raw.secretHash === 'string' && raw.secretHash.trim()
      ? raw.secretHash.trim()
      : null,
  };
}

export interface AccountPort {
  listAccounts(agentId?: AgentKey): Promise<AccountAuthView[]>;
  /**
   * Re-read live auth files into the pool. No upstream quota HTTP.
   * Optional so mocks can omit it.
   */
  reconcileAccounts?(agentId?: AgentKey): Promise<AccountAuthView[]>;
  probeLiveAuth(agentId: AgentKey): Promise<LiveAuthProbe>;
  switchAccount(agentId: AgentKey, accountId: string): Promise<void>;
  undoSwitchAccount(agentId: AgentKey): Promise<boolean>;
  addApiKeyAccount(
    agentId: AgentKey,
    key: string,
    label?: string | null,
    envKey?: string | null,
    productMarker?: string | null,
    extras?: { baseUrl?: string | null; modelId?: string | null } | null,
  ): Promise<Account>;
  /** Update API Key account label and/or key. Omit/empty key keeps the stored secret. */
  updateApiKeyAccount(
    agentId: AgentKey,
    accountId: string,
    opts: { label?: string | null; key?: string | null },
  ): Promise<Account>;
  importCurrentLogin(agentId: AgentKey): Promise<Account>;
  /** Whether any OAuth login option is available for this agent. */
  oauthSupported(agentId: AgentKey): Promise<boolean>;
  /** List OAuth login options (Pi returns multi-provider catalog). */
  listOAuthOptions(agentId: AgentKey): Promise<OAuthLoginOption[]>;
  /** Start loopback PKCE; opens system browser when openBrowser=true. */
  startOAuth(
    agentId: AgentKey,
    openBrowser?: boolean,
    providerKey?: string | null,
  ): Promise<OAuthStartInfo>;
  /** Block until callback or timeout. */
  waitOAuth(state: string, timeoutSecs?: number): Promise<OAuthWaitInfo>;
  /** Exchange code for the given PKCE state and store account. */
  finishOAuth(state: string): Promise<Account>;
  /** Fail an in-flight PKCE or device-code session and release its loopback port. */
  cancelOAuth(state: string): Promise<void>;
  /** Device-code flow (Pi xAI). */
  startDeviceOAuth(
    agentId: AgentKey,
    providerKey: string,
    poolOwned?: boolean,
  ): Promise<DeviceOAuthStartInfo>;
  pollDeviceOAuth(state: string): Promise<DeviceOAuthPollInfo>;
  finishDeviceOAuth(state: string, poolOwned?: boolean): Promise<Account>;
  /**
   * Convenience: start + wait + finish for agents that support OAuth.
   * Prefer start/wait/finish for UI progress. Mock may implement only this.
   */
  completeOAuth(agentId: AgentKey, providerKey?: string | null): Promise<Account>;
  deleteAccount(agentId: AgentKey, accountId: string): Promise<void>;
  refreshToken(agentId: AgentKey, accountId: string): Promise<void>;
  /** Force-refresh upstream 5h/7d quota windows for OAuth (Codex/Claude). */
  refreshQuota?(agentId: AgentKey, accountId: string): Promise<Account>;
}
