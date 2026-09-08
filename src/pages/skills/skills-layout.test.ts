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

  it('uses meta muted type for skill list descriptions', () => {
    const matrix = source('SkillMatrix.tsx');
    expect(matrix).toContain('text-meta text-muted');
    expect(matrix).not.toContain('text-sm text-secondary');
  });

  it('lets empty library install from the empty state and pick a local folder', () => {
    const page = source('index.tsx');
    const panel = source('SkillsLibraryPanel.tsx');
    expect(page).toContain('pickDirectory');
    expect(page).toContain('pickFile');
    expect(page).toContain("t('skills.dialog.sourceLabel')");
    expect(page).toContain("t('skills.dialog.pickFolder')");
    expect(page).toContain("t('skills.dialog.pickZip')");
    expect(page).toContain('installSourceError');
    expect(page).toContain('onEmptyInstall');
    expect(page).toContain('onEmptyMarket');
    expect(panel).toContain("t('skills.empty.goMarket')");
    expect(panel).toContain("t('skills.page.installCta')");
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
