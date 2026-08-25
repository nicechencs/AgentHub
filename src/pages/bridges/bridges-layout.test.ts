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
    expect(list).toContain('DetailsToggle');
    expect(list).toContain('controlsId={detailsId}');
    expect(list).not.toContain("t('routes.collapse')");
    const connections = source('pages/connections/TicketWalletList.tsx');
    expect(connections).toContain('DetailsToggle');
    expect(connections).toContain("t('connections.list.details')");
    const importDialog = source('pages/bridges/ImportRouteDialog.tsx');
    expect(importDialog).toContain('DetailsToggle');
    expect(importDialog).toContain("t('connections.list.details')");
    const detail = source('pages/bridges/RouteDetailPanel.tsx');
    expect(detail).not.toContain("t('routes.collapse')");
    expect(detail).toContain('variant="plain"');
    expect(detail).not.toContain("t('routes.edit.action')");
    expect(list).toContain("t('routes.edit.action')");
  });

  it('puts cancel + save in the inspect header and collapse beside them', () => {
    const shell = source('pages/bridges/dialog-or-side.tsx');
    expect(shell).toContain('headerActions={actions}');
    expect(shell).toContain("t('common.cancel')");
    expect(shell).toContain('variant="secondary"');
    expect(shell).toContain('footer={danger}');
    const panel = source('components/layout/SideInspectPanel.tsx');
    expect(panel).toContain('PanelRightClose');
    expect(panel).toContain('flex h-10');
  });
});
