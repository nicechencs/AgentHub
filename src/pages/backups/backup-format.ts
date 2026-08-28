import type { TranslateFn } from '@/lib/i18n';
import type { AppSettings, BackupKind, BackupMeta } from '@/lib/types';

export function fmtRelativeI18n(iso: string | undefined, t: TranslateFn): string {
  if (!iso) return '—';
  const diff = Date.now() - new Date(iso).getTime();
  const m = Math.floor(diff / 60000);
  if (m < 1) return t('common.relativeJustNow');
  if (m < 60) return t('common.relativeMinutes', { n: m });
  const h = Math.floor(m / 60);
  if (h < 24) return t('common.relativeHours', { n: h });
  return t('common.relativeDays', { n: Math.floor(h / 24) });
}

export function fmtAbsoluteI18n(iso: string, lang: AppSettings['language']): string {
  return new Date(iso).toLocaleString(lang === 'en' ? 'en-US' : 'zh-CN', { hour12: false });
}

/** Visible row title kind: 切换前自动 / 手动备份 / … — never a raw note. */
export function backupTitleKind(kind: BackupKind, t: TranslateFn): string {
  switch (kind) {
    case 'auto-switch':
      return t('settings.backups.kindAutoSwitch');
    case 'manual':
      return t('settings.backups.titleManual');
    case 'pre-uninstall':
      return t('settings.backups.kindPreUninstall');
    case 'pre-restore':
      return t('settings.backups.kindPreRestore');
    case 'pre-skill-uninstall':
      return t('settings.backups.kindPreSkillUninstall');
  }
}

/** Internal notes look like provider/adapter slugs, switch traces, or UUIDs. */
export function isInternalBackupNote(note: string | undefined | null): boolean {
  if (note == null) return true;
  const n = note.trim();
  if (!n) return true;
  if (/before provider switch/i.test(n)) return true;
  if (/after provider (upsert|update)/i.test(n)) return true;
  if (/adapter-bridge/i.test(n)) return true;
  if (/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/i.test(n)) return true;
  if (/[0-9a-f]{16,}/i.test(n)) return true;
  return false;
}

export function backupNoteSubtitle(note: string | undefined | null): string | null {
  if (isInternalBackupNote(note)) return null;
  return note!.trim();
}

export function backupRowTitle(
  bk: Pick<BackupMeta, 'kind' | 'createdAt'>,
  t: TranslateFn,
): string {
  return `${backupTitleKind(bk.kind, t)} · ${fmtRelativeI18n(bk.createdAt, t)}`;
}

export function backupFileLabel(path: string): string {
  const norm = path.replace(/\\/g, '/').trim();
  if (!norm) return path;
  const base = norm.split('/').filter(Boolean).pop();
  return base || path;
}

export function backupFileLabels(files: readonly string[] | undefined): string {
  const names = [...new Set((files ?? []).map(backupFileLabel).filter(Boolean))];
  return names.join(' · ');
}

/** Card title: user note, extracted identity, or file names. */
export function backupCardIdentity(
  bk: Pick<BackupMeta, 'identity' | 'note' | 'files'>,
): string {
  const note = backupNoteSubtitle(bk.note);
  if (note) return note;
  const identity = bk.identity?.trim();
  if (identity) return identity;
  return backupFileLabels(bk.files);
}
