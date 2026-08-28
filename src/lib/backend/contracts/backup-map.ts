import type { AgentId, BackupInspect, BackupKind, BackupMeta } from '@/lib/types';

export interface CoreBackupRecord {
  id: string;
  agentId?: AgentId | null;
  kind: BackupKind;
  path: string;
  files: string[];
  size: number;
  note?: string | null;
  createdAt: string;
  identity?: string | null;
}

export type CoreBackupInspect = BackupInspect;

export interface CoreRestoreResult {
  restored: CoreBackupRecord;
  preRestore?: CoreBackupRecord | null;
  restoredPaths: string[];
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
