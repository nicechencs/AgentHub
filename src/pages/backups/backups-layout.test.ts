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
});
