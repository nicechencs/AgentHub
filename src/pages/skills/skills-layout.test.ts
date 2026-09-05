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
    expect(page).toContain('PageHeader');
    expect(page).toContain('pageRhythm.chromeActions');
    expect(page).toContain("t('skills.preview.resizeAria')");
    expect(page).toContain('<SkillMarkdownPreviewPanel');
    expect(page).not.toContain('previewShellMounted');
    expect(page).not.toContain('onPreviewResizeStart');
    expect(page).toContain('followInspectOpen');
    expect(page).toContain('preview.expanded');
    expect(source('SkillMatrix.tsx')).toContain('onOpen={onFollow');
    expect(source('SkillsProjectPanel.tsx')).toContain('onOpen={onFollow');
  });

  it('keeps the project picker trigger to a single line', () => {
    const panel = source('SkillsProjectPanel.tsx');
    expect(panel).toContain('SelectValue');
    expect(panel).toContain('description={option.subtitle}');
    expect(panel).toContain('{option.label}');
    expect(panel).not.toContain('flex min-w-0 flex-col');
  });

  it('keeps the install action on the tab row in the left split column', () => {
    const page = source('index.tsx');
    const installStart = page.indexOf('setInstallOpen(true)');
    const listStart = page.indexOf('<Tabs ');

    expect(listStart).toBeGreaterThan(0);
    expect(installStart).toBeGreaterThan(listStart);
    expect(page).toContain('pageRhythm.chromeActions');
    expect(page).not.toContain('header={(');
  });
});
