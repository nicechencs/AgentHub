import type { Account, AgentId } from '@/lib/types';
import { normalizeAuthHealth, type AuthHealth } from './auth-state';

/** PKCE start result from backend. */
export interface OAuthStartInfo {
  state: string;
  authorizeUrl: string;
  redirectUri: string;
  agentId: AgentId;
  /** Pi multi-provider key when applicable. */
  providerKey?: string | null;
  browserOpened: boolean;
}

export interface OAuthWaitInfo {
  state: string;
  agentId: AgentId;
  status: 'waiting' | 'callbackReceived' | 'succeeded' | 'failed';
  error?: string | null;
}

export type OAuthFlowKind = 'pkce' | 'deviceCode';

/** One selectable OAuth login target (Pi has multiple). */
export interface OAuthLoginOption {
  id: string;
  agentId: AgentId;
  label: string;
  description: string;
  flow: OAuthFlowKind;
  authJsonKey?: string | null;
}

export interface DeviceOAuthStartInfo {
  state: string;
  agentId: AgentId;
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
  agent: AgentId;
  kind?: string | null;
  summary: string;
  hasCredentials: boolean;
  /** Non-secret live-file revision used to detect external token rotation. */
  revision?: string | null;
  /** Optional backend verification result (old backends omit it). */
  health?: AuthHealth;
  /** Redacted source label (settings/auth file/live/etc.). */
  source?: string | null;
}

/** Normalized probe consumed by browser pages; keeps agentId for old callers. */
export interface LiveAuthProbe {
  agentId: AgentId;
  kind?: string | null;
  summary: string;
  hasCredentials: boolean;
  /** Non-secret live-file revision used to detect external token rotation. */
  revision?: string | null;
  health?: AuthHealth;
  source?: string | null;
}

/** Accept both current core AuthState (`agent`) and legacy JS probe (`agentId`). */
export function normalizeAuthState(
  raw: Partial<AuthState> & { agentId?: AgentId },
  fallbackAgentId: AgentId,
): LiveAuthProbe {
  const agentId = raw.agentId ?? raw.agent ?? fallbackAgentId;
  return {
    agentId,
    kind: raw.kind ?? null,
    summary: typeof raw.summary === 'string' ? raw.summary : '',
    hasCredentials: raw.hasCredentials === true,
    revision: raw.revision ?? null,
    health: normalizeAuthHealth(raw.health),
    source: raw.source ?? null,
  };
}

export interface AccountPort {
  listAccounts(agentId?: AgentId): Promise<Account[]>;
  probeLiveAuth(agentId: AgentId): Promise<LiveAuthProbe>;
  switchAccount(agentId: AgentId, accountId: string): Promise<void>;
  undoSwitchAccount(agentId: AgentId): Promise<boolean>;
  addApiKeyAccount(
    agentId: AgentId,
    key: string,
    label?: string | null,
    envKey?: string | null,
  ): Promise<Account>;
  /** Update API Key account label and/or key. Omit/empty key keeps the stored secret. */
  updateApiKeyAccount(
    agentId: AgentId,
    accountId: string,
    opts: { label?: string | null; key?: string | null },
  ): Promise<Account>;
  importCurrentLogin(agentId: AgentId): Promise<Account>;
  /** Whether any OAuth login option is available for this agent. */
  oauthSupported(agentId: AgentId): Promise<boolean>;
  /** List OAuth login options (Pi returns multi-provider catalog). */
  listOAuthOptions(agentId: AgentId): Promise<OAuthLoginOption[]>;
  /** Start loopback PKCE; opens system browser when openBrowser=true. */
  startOAuth(
    agentId: AgentId,
    openBrowser?: boolean,
    providerKey?: string | null,
  ): Promise<OAuthStartInfo>;
  /** Block until callback or timeout. */
  waitOAuth(state: string, timeoutSecs?: number): Promise<OAuthWaitInfo>;
  /** Exchange code for the given PKCE state and store account. */
  finishOAuth(state: string): Promise<Account>;
  /** Device-code flow (Pi xAI). */
  startDeviceOAuth(agentId: AgentId, providerKey: string): Promise<DeviceOAuthStartInfo>;
  pollDeviceOAuth(state: string): Promise<DeviceOAuthPollInfo>;
  finishDeviceOAuth(state: string): Promise<Account>;
  /**
   * Convenience: start + wait + finish for agents that support OAuth.
   * Prefer start/wait/finish for UI progress. Mock may implement only this.
   */
  completeOAuth(agentId: AgentId, providerKey?: string | null): Promise<Account>;
  deleteAccount(agentId: AgentId, accountId: string): Promise<void>;
  refreshToken(agentId: AgentId, accountId: string): Promise<void>;
  /** Force-refresh upstream 5h/7d quota windows for OAuth (Codex/Claude). */
  refreshQuota?(agentId: AgentId, accountId: string): Promise<Account>;
}
