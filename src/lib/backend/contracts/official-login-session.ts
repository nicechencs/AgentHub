/**
 * Unified official-login session: one model the wait page talks to.
 * PKCE and device-code stay the only grant types; this file maps both.
 */
import type { MessageKey } from '@/lib/i18n';
import type { Account, AgentId } from '@/lib/types';
import type {
  DeviceOAuthPollInfo,
  DeviceOAuthStartInfo,
  OAuthLoginOption,
  OAuthStartInfo,
  OAuthWaitInfo,
} from './account-port';
import { OAUTH_WAIT_TIMEOUT_SECS } from './oauth-constants';

export type OfficialLoginFlow = 'pkce' | 'deviceCode';

/** User-visible wait-page phase. Protocol statuses collapse into these. */
export type OfficialLoginPhase = 'waiting' | 'ready' | 'failed' | 'expired' | 'cancelled';

export type OfficialLoginCopyId =
  | 'claude'
  | 'codex'
  | 'grok'
  | 'piAnthropic'
  | 'piCodex'
  | 'piXai';

export type OfficialLoginDialogStep =
  | 'check'
  | 'pick'
  | 'start'
  | 'waiting'
  | 'done'
  | 'unavailable'
  | 'error';

export type OfficialLoginFooterKind = 'close' | 'cancelWait' | 'retry' | 'success';

/** Known Pi keys that AgentHub does not implement. Must not be clickable. */
export const UNIMPLEMENTED_PI_OAUTH_KEYS = [
  'github-copilot',
  'openrouter',
  'kimi-coding',
  'radius',
] as const;

/** Implemented official-login option ids, keyed by agent. */
export const IMPLEMENTED_OFFICIAL_LOGIN_IDS: Readonly<Record<string, readonly string[]>> = {
  claude: ['claude'],
  codex: ['codex'],
  grok: ['xai'],
  pi: ['anthropic', 'openai-codex', 'xai'],
};

export interface OfficialLoginSession {
  /** Backend session id. Never show this in the UI. */
  sessionId: string;
  agentId: AgentId;
  optionId: string;
  flow: OfficialLoginFlow;
  authorizeUrl?: string | null;
  redirectUri?: string | null;
  browserOpened?: boolean;
  userCode?: string | null;
  verificationUri?: string | null;
  verificationUriComplete?: string | null;
  intervalSecs: number;
  expiresInSecs: number;
}

export interface OfficialLoginPoll {
  phase: OfficialLoginPhase;
  error?: string | null;
}

export interface OfficialLoginSuccessView {
  /** Primary identity line. Null when the login returned none. */
  title: string | null;
  subscription: string | null;
  /** Extra identity only when it was returned and differs from title. */
  identity: string | null;
}

const LOOPBACK_HOSTS = new Set(['127.0.0.1', 'localhost', '::1', '[::1]']);

const GENERIC_LOGIN_LABEL =
  /(?:\(oauth\)$|-oauth$|[·\s]oauth$|^pi-auth$|^官方登录$|^official login$)/i;

/** User-facing strings must not contain these internals. */
export const OFFICIAL_LOGIN_INTERNAL_COPY =
  /PKCE|loopback|ticket|wallet|auth\.json|state:\s|~\s*\/\s*\.pi|票|钱包|真源|投影|Ticket|Adapter/i;

export function officialLoginCopyLeaksInternals(text: string): boolean {
  return OFFICIAL_LOGIN_INTERNAL_COPY.test(text);
}

export function officialLoginCopyId(
  agentId: string,
  optionId: string,
): OfficialLoginCopyId | null {
  const agent = agentId.trim().toLowerCase();
  const id = optionId.trim().toLowerCase();
  if (agent === 'claude' && (id === 'claude' || id === 'anthropic')) return 'claude';
  if (agent === 'codex' && (id === 'codex' || id === 'openai-codex' || id === 'openai')) {
    return 'codex';
  }
  if (agent === 'grok' && (id === 'xai' || id === 'grok')) return 'grok';
  if (agent === 'pi') {
    if (id === 'anthropic' || id === 'claude') return 'piAnthropic';
    if (id === 'openai-codex' || id === 'codex' || id === 'openai') return 'piCodex';
    if (id === 'xai' || id === 'grok') return 'piXai';
  }
  return null;
}

const OPTION_LABEL_KEY: Record<OfficialLoginCopyId, MessageKey> = {
  claude: 'connect.oauth.option.claude.label',
  codex: 'connect.oauth.option.codex.label',
  grok: 'connect.oauth.option.grok.label',
  piAnthropic: 'connect.oauth.option.piAnthropic.label',
  piCodex: 'connect.oauth.option.piCodex.label',
  piXai: 'connect.oauth.option.piXai.label',
};

const OPTION_DESCRIPTION_KEY: Record<OfficialLoginCopyId, MessageKey> = {
  claude: 'connect.oauth.option.claude.description',
  codex: 'connect.oauth.option.codex.description',
  grok: 'connect.oauth.option.grok.description',
  piAnthropic: 'connect.oauth.option.piAnthropic.description',
  piCodex: 'connect.oauth.option.piCodex.description',
  piXai: 'connect.oauth.option.piXai.description',
};

export function officialLoginOptionLabelKey(copyId: OfficialLoginCopyId): MessageKey {
  return OPTION_LABEL_KEY[copyId];
}

export function officialLoginOptionDescriptionKey(copyId: OfficialLoginCopyId): MessageKey {
  return OPTION_DESCRIPTION_KEY[copyId];
}

export function isUnimplementedPiOauth(optionId: string): boolean {
  const id = optionId.trim().toLowerCase();
  return (UNIMPLEMENTED_PI_OAUTH_KEYS as readonly string[]).includes(id);
}

export function isImplementedOfficialLogin(agentId: string, optionId: string): boolean {
  if (isUnimplementedPiOauth(optionId)) return false;
  return officialLoginCopyId(agentId, optionId) != null;
}

export function presentOfficialLoginOptions<T extends { agentId: string; id: string }>(
  options: readonly T[],
): T[] {
  return options.filter((opt) => isImplementedOfficialLogin(opt.agentId, opt.id));
}

export function officialLoginAdapter(flow: OAuthLoginOption['flow'] | OfficialLoginFlow): OfficialLoginFlow {
  return flow === 'deviceCode' ? 'deviceCode' : 'pkce';
}

export function mapPkceWaitStatus(status: OAuthWaitInfo['status']): OfficialLoginPhase {
  switch (status) {
    case 'waiting':
      return 'waiting';
    case 'callbackReceived':
    case 'succeeded':
      return 'ready';
    case 'failed':
      return 'failed';
    default:
      return 'failed';
  }
}

export function mapDevicePollStatus(status: DeviceOAuthPollInfo['status']): OfficialLoginPhase {
  switch (status) {
    case 'pending':
    case 'slowDown':
    case 'completing':
      return 'waiting';
    case 'complete':
      return 'ready';
    case 'failed':
      return 'failed';
    case 'expired':
      return 'expired';
    default:
      return 'failed';
  }
}

export function officialLoginShouldFinish(phase: OfficialLoginPhase): boolean {
  return phase === 'ready';
}

export function officialLoginShouldKeepPolling(phase: OfficialLoginPhase): boolean {
  return phase === 'waiting';
}

export function sessionFromPkceStart(
  start: OAuthStartInfo,
  optionId: string,
): OfficialLoginSession {
  return {
    sessionId: start.state,
    agentId: start.agentId,
    optionId: start.providerKey?.trim() ? start.providerKey : optionId,
    flow: 'pkce',
    authorizeUrl: start.authorizeUrl,
    redirectUri: start.redirectUri,
    browserOpened: start.browserOpened,
    intervalSecs: 0,
    expiresInSecs: OAUTH_WAIT_TIMEOUT_SECS,
  };
}

export function sessionFromDeviceStart(start: DeviceOAuthStartInfo): OfficialLoginSession {
  return {
    sessionId: start.state,
    agentId: start.agentId,
    optionId: start.providerKey,
    flow: 'deviceCode',
    userCode: start.userCode,
    verificationUri: start.verificationUri,
    verificationUriComplete: start.verificationUriComplete,
    intervalSecs: start.intervalSecs,
    expiresInSecs: start.expiresInSecs,
  };
}

export function officialLoginRetryStep(optionCount: number): Extract<OfficialLoginDialogStep, 'pick' | 'start'> {
  return optionCount > 1 ? 'pick' : 'start';
}

export function officialLoginFooter(
  step: OfficialLoginDialogStep,
  waitingActive: boolean,
): OfficialLoginFooterKind {
  if (step === 'done') return 'success';
  if (step === 'error') return 'retry';
  if (step === 'waiting' || waitingActive) return 'cancelWait';
  return 'close';
}

function isGenericOfficialLoginLabel(label: string): boolean {
  const trimmed = label.trim();
  if (!trimmed) return true;
  return GENERIC_LOGIN_LABEL.test(trimmed);
}

/**
 * Success card uses only fields the login actually returned.
 * Never invents email, last4, or identity from a tail / placeholder.
 */
export function officialLoginSuccessView(
  account: Pick<Account, 'email' | 'identityLabel' | 'label' | 'subscription' | 'secretTail'>,
): OfficialLoginSuccessView {
  const email = account.email?.trim() || null;
  const identityRaw = account.identityLabel?.trim() || null;
  const label = account.label?.trim() || null;
  const subscription = account.subscription?.trim() || null;

  const identity =
    identityRaw && !isGenericOfficialLoginLabel(identityRaw) ? identityRaw : null;
  const usableLabel = label && !isGenericOfficialLoginLabel(label) ? label : null;
  const title = email ?? identity ?? usableLabel;

  return {
    title,
    subscription,
    identity: identity && identity !== title ? identity : null,
  };
}

function normalizeCallbackPath(path: string): string {
  if (!path) return '/';
  return path.length > 1 && path.endsWith('/') ? path.slice(0, -1) : path;
}

/** Loopback PKCE callback pasted from the browser. Rejects arbitrary URLs. */
export function validateManualCallbackUrl(
  raw: string,
  redirectUri: string,
  expectedState: string,
): { ok: true; href: string } | { ok: false } {
  const trimmed = raw.trim();
  if (!trimmed || !redirectUri.trim() || !expectedState.trim()) return { ok: false };
  let pasted: URL;
  let expected: URL;
  try {
    pasted = new URL(trimmed);
    expected = new URL(redirectUri);
  } catch {
    return { ok: false };
  }
  if (pasted.protocol !== 'http:') return { ok: false };
  if (!LOOPBACK_HOSTS.has(pasted.hostname.toLowerCase())) return { ok: false };
  if (pasted.port !== expected.port) return { ok: false };
  if (normalizeCallbackPath(pasted.pathname) !== normalizeCallbackPath(expected.pathname)) {
    return { ok: false };
  }
  if (pasted.searchParams.get('state') !== expectedState) return { ok: false };
  const code = pasted.searchParams.get('code')?.trim();
  const error = pasted.searchParams.get('error')?.trim();
  if (!code && !error) return { ok: false };
  return { ok: true, href: pasted.href };
}
