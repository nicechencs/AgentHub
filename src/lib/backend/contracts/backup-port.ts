import type { AgentKey, BackupInspect, BackupMeta } from '@/lib/types';

export interface BackupPort {
  listBackups(agentId?: AgentKey): Promise<BackupMeta[]>;
  inspectBackup(backupId: string): Promise<BackupInspect>;
  createBackup(agentId: AgentKey, note?: string): Promise<BackupMeta>;
  restoreBackup(backupId: string): Promise<void>;
  deleteBackup(backupId: string): Promise<void>;
}
