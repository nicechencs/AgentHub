/**
 * Official-login session façade.
 * The wait page calls start / poll / finish / cancel.
 * PKCE and device-code stay the adapters underneath.
 */
import {
  cancelOAuth,
  finishDeviceOAuth,
  finishOAuth,
  pollDeviceOAuth,
  startDeviceOAuth,
  startOAuth,
  waitOAuth,
} from '@/lib/api/account';
import {
  officialLoginAdapter,
  mapDevicePollStatus,
  mapPkceWaitStatus,
  sessionFromDeviceStart,
  sessionFromPkceStart,
  type OfficialLoginPoll,
  type OfficialLoginSession,
} from '@/lib/backend/contracts/official-login-session';
import { OAUTH_WAIT_TIMEOUT_SECS } from '@/lib/backend/contracts/oauth-constants';
import type { OAuthLoginOption } from '@/lib/backend/contracts/account-port';
import type { Account, AgentId } from '@/lib/types';

export type { OfficialLoginPoll, OfficialLoginSession };

export async function startOfficialLogin(
  agentId: AgentId,
  option: Pick<OAuthLoginOption, 'id' | 'flow'>,
  openBrowser = true,
): Promise<OfficialLoginSession> {
  if (officialLoginAdapter(option.flow) === 'deviceCode') {
    const start = await startDeviceOAuth(agentId, option.id);
    return sessionFromDeviceStart(start);
  }
  const start = await startOAuth(agentId, openBrowser, option.id);
  return sessionFromPkceStart(start, option.id);
}

export async function pollOfficialLogin(
  session: OfficialLoginSession,
  timeoutSecs = OAUTH_WAIT_TIMEOUT_SECS,
): Promise<OfficialLoginPoll> {
  if (session.flow === 'deviceCode') {
    const poll = await pollDeviceOAuth(session.sessionId);
    return { phase: mapDevicePollStatus(poll.status), error: poll.error ?? null };
  }
  const wait = await waitOAuth(session.sessionId, timeoutSecs);
  return { phase: mapPkceWaitStatus(wait.status), error: wait.error ?? null };
}

export async function finishOfficialLogin(session: OfficialLoginSession): Promise<Account> {
  if (session.flow === 'deviceCode') return finishDeviceOAuth(session.sessionId);
  return finishOAuth(session.sessionId);
}

export async function cancelOfficialLogin(
  session: OfficialLoginSession | string | null | undefined,
): Promise<void> {
  const id = typeof session === 'string' ? session : session?.sessionId;
  if (!id) return;
  await cancelOAuth(id);
}
