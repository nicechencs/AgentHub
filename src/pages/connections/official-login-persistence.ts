import { toLoginSaveResult, type OfficialLoginPersistence } from '@/components/login-kernel';
import {
  cancelOfficialLogin,
  finishOfficialLogin,
  startOfficialLogin,
} from '@/lib/api/official-login';
import type { Account } from '@/lib/types';

/** Connections official login: global account, not a pool-owned row. */
export function createConnectionsOfficialLoginPersistence(options?: {
  onAccount?: (account: Account) => void;
}): OfficialLoginPersistence {
  return {
    start: (agentId, option) => startOfficialLogin(agentId, option, false),
    finish: async (session) => {
      const account = await finishOfficialLogin(session);
      options?.onAccount?.(account);
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
