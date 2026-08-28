/**
 * Backup API façade — delegates to app runtime backend.
 */
import { getBackend } from '@/app/runtime';
import type { AgentId, BackupInspect, BackupMeta } from '@/lib/types';

export type { CoreBackupRecord, CoreRestoreResult } from '@/lib/backend/contracts/backup-map';
export { mapCoreBackup } from '@/lib/backend/contracts/backup-map';

export async function listBackups(agentId?: AgentId): Promise<BackupMeta[]> {
  return getBackend().backup.listBackups(agentId);
}

export async function inspectBackup(backupId: string): Promise<BackupInspect> {
  return getBackend().backup.inspectBackup(backupId);
}

export async function createBackup(agentId: AgentId, note?: string): Promise<BackupMeta> {
  return getBackend().backup.createBackup(agentId, note);
}

export async function restoreBackup(backupId: string): Promise<void> {
  return getBackend().backup.restoreBackup(backupId);
}

export async function deleteBackup(backupId: string): Promise<void> {
  return getBackend().backup.deleteBackup(backupId);
}
