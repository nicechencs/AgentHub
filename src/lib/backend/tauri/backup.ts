import type { BackupPort } from '@/lib/backend/contracts';
import {
  mapCoreBackup,
  type CoreBackupRecord,
  type CoreRestoreResult,
} from '@/lib/backend/contracts/backup-map';
import { invoke } from './invoke';

export function createTauriBackupPort(): BackupPort {
  return {
    async listBackups(agentId) {
      const rows = await invoke<CoreBackupRecord[]>('list_backups', {
        agentId: agentId ?? null,
      });
      return rows.map(mapCoreBackup).filter((b): b is NonNullable<typeof b> => b !== null);
    },

    async createBackup(agentId, note) {
      const row = await invoke<CoreBackupRecord>('create_backup', {
        agentId,
        note: note ?? null,
      });
      const mapped = mapCoreBackup(row);
      if (!mapped) throw new Error('备份记录缺少 agentId');
      return mapped;
    },

    async restoreBackup(backupId) {
      await invoke<CoreRestoreResult>('restore_backup', { backupId });
    },

    async deleteBackup(backupId) {
      await invoke('delete_backup', { backupId });
    },
  };
}
