import type { AgentKey, BackupInspect, BackupKind, BackupMeta } from '@/lib/types';

export interface CoreBackupRecord {
  id: string;
  agentId?: AgentKey | null;
  kind: BackupKind;
  path: string;
  files: string[];
  size: number;
  note?: string | null;
  createdAt: string;
  identity?: string | null;
}

export type CoreBackupInspect = BackupInspect;

export interface CoreSkippedDeletion {
  path: string;
  reason: string;
}

export interface CoreRestoreResult {
  restored: CoreBackupRecord;
  preRestore?: CoreBackupRecord | null;
  restoredPaths: string[];
  skippedDeletions?: CoreSkippedDeletion[];
}

export function mapCoreRestoreResult(result: CoreRestoreResult): CoreRestoreResult {
  return {
    ...result,
    restoredPaths: result.restoredPaths ?? [],
    skippedDeletions: result.skippedDeletions ?? [],
  };
}

export function restoreHasDeleteFailures(
  result: Pick<CoreRestoreResult, 'skippedDeletions'>,
): boolean {
  return (result.skippedDeletions ?? []).some((item) => item.reason === 'delete_failed');
}

export function mapCoreBackup(b: CoreBackupRecord): BackupMeta | null {
  if (!b.agentId) return null;
  return {
    id: b.id,
    agentId: b.agentId,
    kind: b.kind,
    createdAt: b.createdAt,
    files: [...(b.files ?? [])],
    sizeBytes: b.size ?? 0,
    note: b.note ?? undefined,
    identity: b.identity?.trim() || undefined,
  };
}
