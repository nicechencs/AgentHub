import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const dir = path.dirname(fileURLToPath(import.meta.url));

function source(name: string): string {
  return readFileSync(path.join(dir, name), 'utf8');
}

describe('projects split layout', () => {
  it('opens session preview in the shared workbench inspect pane', () => {
    const page = source('index.tsx');
    expect(page).toContain('WorkbenchSplitPage');
    expect(page).toContain('useSideSplit');
    expect(page).toContain("size=\"compact\"");
    expect(page).toContain("t('projects.preview.resizeAria')");
    expect(page).toContain('listOverflowX="hidden"');
    expect(page).toContain('<ProjectConversationPreviewPanel');
    expect(page).not.toContain('useProjectPreview');
    expect(page).not.toContain('previewShellMounted');
  });
});
