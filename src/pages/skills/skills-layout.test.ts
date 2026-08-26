import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const dir = path.dirname(fileURLToPath(import.meta.url));

function source(name: string): string {
  return readFileSync(path.join(dir, name), 'utf8');
}

describe('skills split layout', () => {
  it('opens preview in the shared workbench inspect pane', () => {
    const page = source('index.tsx');
    expect(page).toContain('WorkbenchSplitPage');
    expect(page).toContain('useSideSplit');
    expect(page).toContain("size=\"compact\"");
    expect(page).toContain("t('skills.preview.resizeAria')");
    expect(page).toContain('<SkillMarkdownPreviewPanel');
    expect(page).not.toContain('previewShellMounted');
    expect(page).not.toContain('onPreviewResizeStart');
  });

  it('keeps the page header and install action in the left split column', () => {
    const page = source('index.tsx');
    const headerStart = page.indexOf('header={(');
    const installStart = page.indexOf('setInstallOpen(true)');
    const listStart = page.indexOf('<Tabs ');

    expect(headerStart).toBeGreaterThanOrEqual(0);
    expect(installStart).toBeGreaterThan(headerStart);
    expect(installStart).toBeLessThan(listStart);
  });
});
