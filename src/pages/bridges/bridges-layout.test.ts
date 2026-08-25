import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const dir = path.dirname(fileURLToPath(import.meta.url));
const srcRoot = path.resolve(dir, '../..');

function source(rel: string): string {
  return readFileSync(path.join(srcRoot, rel), 'utf8');
}

describe('routes layout wiring', () => {
  it('uses the Skills/Projects workbench split for inspect panes', () => {
    const page = source('pages/bridges/index.tsx');
    expect(page).toContain('WorkbenchSplitPage');
    expect(page).toContain('size="compact"');
    expect(page).toContain("t('common.resizeSidePanel')");
    expect(page).toContain('useSideSplit');
    expect(page).not.toContain('flex items-start gap-3');
  });

  it('uses the same 详情 toggle as Connections (label + chevron, not 收起)', () => {
    const list = source('pages/bridges/AdapterProfilesList.tsx');
    expect(list).toContain("t('routes.detail')");
    expect(list).toContain('ChevronDown');
    expect(list).toContain('aria-controls={detailsId}');
    expect(list).not.toContain("t('routes.collapse')");
    const detail = source('pages/bridges/RouteDetailPanel.tsx');
    expect(detail).not.toContain("t('routes.collapse')");
    expect(detail).toContain('variant="plain"');
  });

  it('puts save in the inspect header and collapse instead of cancel', () => {
    const shell = source('pages/bridges/dialog-or-side.tsx');
    expect(shell).toContain('headerActions={primary}');
    expect(shell).toContain('footer={danger}');
    const panel = source('components/layout/SideInspectPanel.tsx');
    expect(panel).toContain('PanelRightClose');
    expect(panel).toContain('flex h-10');
  });
});
