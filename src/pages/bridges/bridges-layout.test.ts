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

  it('uses a plain 详情 button on Routes (no chevron; Connections still expands)', () => {
    const list = source('pages/bridges/AdapterProfilesList.tsx');
    expect(list).toContain("t('routes.detail')");
    expect(list).not.toContain('DetailsToggle');
    expect(list).not.toContain('ChevronDown');
    expect(list).not.toContain("t('routes.collapse')");
    expect(list).not.toContain('RouteDetailPanel');
    const connections = source('pages/connections/TicketWalletList.tsx');
    expect(connections).toContain('DetailsToggle');
    expect(connections).toContain("t('connections.list.details')");
    const importDialog = source('pages/bridges/ImportRouteDialog.tsx');
    expect(importDialog).toContain('DetailsToggle');
    expect(importDialog).toContain("t('connections.list.details')");
    const detail = source('pages/bridges/RouteDetailPanel.tsx');
    expect(detail).toContain('asPanel');
    expect(detail).toContain('DialogOrSide');
    expect(detail).toContain("t('routes.edit.action')");
    expect(list).toContain("t('routes.edit.action')");
    expect(list).toContain('variant="outline"');
  });

  it('opens edit and detail in the same right-hand inspect pane', () => {
    const page = source('pages/bridges/index.tsx');
    expect(page).toContain("{ kind: 'edit'; profile: AdapterProfile }");
    expect(page).toContain("{ kind: 'detail'; profile: AdapterProfile }");
    expect(page).toContain('onShowDetail');
    expect(page).toContain("kind: 'detail'");
    expect(page).toContain('<RouteDetailPanel');
    expect(page).toContain('asPanel');
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
