import { toLoginSaveResult, type OfficialLoginPersistence } from '@/components/login-kernel';
import type { RouteSchedulePolicy } from '@/lib/backend/contracts/adapter';
import {
  cancelOfficialLogin,
  finishOfficialLogin,
  startOfficialLogin,
} from '@/lib/api/official-login';
import type { Account } from '@/lib/types';

/** Pool official login stays pool-owned and keeps the create-time schedule. */
export function createPoolOfficialLoginPersistence(options: {
  schedulePolicy: () => RouteSchedulePolicy;
  onAccount?: (account: Account) => void;
}): OfficialLoginPersistence {
  return {
    start: (agentId, option) => startOfficialLogin(agentId, option, false, true),
    finish: async (session) => {
      const account = await finishOfficialLogin(session, true, options.schedulePolicy());
      options.onAccount?.(account);
      return toLoginSaveResult({
        kind: account.kind === 'apikey' ? 'apikey' : 'oauth',
        mutation: 'created',
        sourceKind: 'account',
        sourceId: account.id,
        agentId: account.agentId,
      });
    },
    cancel: (session) => cancelOfficialLogin(session),
  };
}
