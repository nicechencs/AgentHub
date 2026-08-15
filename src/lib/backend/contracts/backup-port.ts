import type { AgentId, BackupMeta } from '@/lib/types';

export interface BackupPort {
  listBackups(agentId?: AgentId): Promise<BackupMeta[]>;
  createBackup(agentId: AgentId, note?: string): Promise<BackupMeta>;
  restoreBackup(backupId: string): Promise<void>;
  deleteBackup(backupId: string): Promise<void>;
  exportBackup(backupId: string): Promise<void>;
}
