import type {
  ConnectionTrashItem,
  TrashPort,
} from '@/lib/backend/contracts/ports';
import { mapCoreAccount, type CoreAccount } from '@/lib/backend/contracts/account-map';
import { mapCoreProvider, type CoreProvider } from '@/lib/backend/contracts/provider-map';
import { invoke } from './invoke';
import { logger } from '@/lib/logger';

const log = logger.scope('backend:trash');

type CoreTrashItem = Omit<ConnectionTrashItem, 'account' | 'provider'> & {
  account?: CoreAccount;
  provider?: CoreProvider;
};

function mapTrashItem(item: CoreTrashItem): ConnectionTrashItem {
  return {
    ...item,
    account: item.account ? mapCoreAccount(item.account) : undefined,
    provider: item.provider ? mapCoreProvider(item.provider) : undefined,
  };
}

export function createTauriTrashPort(): TrashPort {
  return {
    async list(agentId) {
      try {
        const rows = await invoke<CoreTrashItem[]>('list_connection_trash', {
          agentId: agentId ?? null,
        });
        return rows.map(mapTrashItem);
      } catch (e) {
        log.error('list_connection_trash failed', e);
        throw e;
      }
    },

    async restore(id) {
      try {
        await invoke('restore_connection_trash', { id });
      } catch (e) {
        log.error('restore_connection_trash failed', e);
        throw e;
      }
    },

    async permanentlyDelete(id) {
      try {
        await invoke('delete_connection_trash', { id });
      } catch (e) {
        log.error('delete_connection_trash failed', e);
        throw e;
      }
    },
  };
}
