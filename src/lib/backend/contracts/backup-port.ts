import type { AgentId, BackupInspect, BackupMeta } from '@/lib/types';

export interface BackupPort {
  listBackups(agentId?: AgentId): Promise<BackupMeta[]>;
  inspectBackup(backupId: string): Promise<BackupInspect>;
  createBackup(agentId: AgentId, note?: string): Promise<BackupMeta>;
  restoreBackup(backupId: string): Promise<void>;
  deleteBackup(backupId: string): Promise<void>;
}
