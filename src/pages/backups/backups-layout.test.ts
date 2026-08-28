import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const dir = path.dirname(fileURLToPath(import.meta.url));

function source(name: string): string {
  return readFileSync(path.join(dir, name), 'utf8');
}

describe('backups layout wiring', () => {
  it('confirms backup delete in a dialog before calling deleteBackup', () => {
    const panel = source('BackupsPanel.tsx');
    expect(panel).toContain('setDeleteTarget');
    expect(panel).toContain("t('settings.backups.deleteTitle')");
    expect(panel).toContain('variant="danger"');
    expect(panel).not.toContain('onClick={() => void handleDelete(bk)}');
    expect(panel).toContain("t('settings.backups.confirmDelete')");
  });

  it('uses compact list rows and opens inspectBackup details', () => {
    const panel = source('BackupsPanel.tsx');
    const detail = source('backup-detail-panel.tsx');
    expect(panel).toContain('ListRow');
    expect(panel).toContain('backupCardIdentity');
    expect(panel).not.toContain('FILE_PREVIEW');
    expect(panel).toContain('<BackupDetailPanel');
    expect(detail).toContain('inspectBackup');
    expect(detail).toContain("t('settings.backups.filesTitle')");
    expect(detail).toContain("t('settings.backups.noTextContent')");
    expect(detail).toContain('openPathInFileManager');
  });

  it('opens backup details in the right-hand inspect pane', () => {
    const panel = source('BackupsPanel.tsx');
    const detail = source('backup-detail-panel.tsx');
    expect(panel).toContain('WorkbenchSplitPage');
    expect(panel).toContain('useSideSplit');
    expect(panel).toContain("t('common.resizeSidePanel')");
    expect(panel).toContain('inspect.open(bk.id)');
    expect(detail).toContain('InspectSurface');
    expect(detail).toContain('asPanel');
    expect(detail).toContain('showCancel={false}');
    expect(detail).toContain('ConfigFileCard');
    expect(detail).toContain('data-backup-detail');
  });
});
