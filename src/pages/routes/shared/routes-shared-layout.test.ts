import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const dir = path.dirname(fileURLToPath(import.meta.url));
const srcRoot = path.resolve(dir, '../../..');

function source(rel: string): string {
  return readFileSync(path.join(srcRoot, rel), 'utf8');
}

describe('routes layout wiring', () => {
  it('uses the Skills/Projects workbench split for inspect panes', () => {
    const page = source('pages/routes/pool/index.tsx');
    expect(page).toContain('WorkbenchSplitPage');
    expect(page).toContain('PageHeader');
    expect(page).toContain('pageRhythm.chromeActions');
    expect(page).toContain("t('common.resizeSidePanel')");
    expect(page).toContain('useSideSplit');
    expect(page).not.toContain('flex items-start gap-3');
  });

  it('edits pool API keys in the login detail pane', () => {
    const page = source('pages/routes/pool/index.tsx');
    const detail = source('pages/routes/pool/PoolAuthorizationDetail.tsx');
    expect(page).toContain('editTarget');
    expect(page).not.toContain('setApiEdit');
    expect(page).not.toContain('ApiAccessDialog');
    expect(detail).toContain('ApiAccessForm');
    expect(detail).toContain('layout="inline"');
    expect(page).not.toContain('ProviderEditDialog');
    expect(page).not.toContain('ApiKeyAccountDialog');
  });

  it('puts fleet summary or orphan lead on the same chrome row as pool add actions', () => {
    const page = source('pages/routes/pool/index.tsx');
    const chromeStart = page.indexOf('pageRhythm.chromeRow');
    const listStart = page.indexOf('pageRhythm.stackDense');
    expect(chromeStart).toBeGreaterThan(0);
    expect(listStart).toBeGreaterThan(chromeStart);
    const chrome = page.slice(chromeStart, listStart);
    expect(chrome).toContain('orphanOnly');
    expect(chrome).toContain('routes.pool.page.chromeHint');
    expect(chrome).toContain('PoolAddButtons');
    expect(chrome).toContain('pageRhythm.chromeActions');
    expect(source('pages/routes/pool/PoolAddButtons.tsx')).toContain('size="sm"');
    const list = page.slice(listStart);
    expect(list).not.toContain('routes.pool.page.chromeHint');
    expect(list).toContain('first={orphanOnly}');
    expect(list).toContain('title={orphanOnly ? undefined');
  });

  it('uses a plain 详情 button on Routes and Connections (no chevron)', () => {
    const list = source('pages/routes/shared/AdapterProfilesList.tsx');
    expect(list).toContain("t('routes.detail')");
    expect(list).not.toContain('DetailsToggle');
    expect(list).not.toContain('ChevronDown');
    expect(list).not.toContain("t('routes.collapse')");
    expect(list).not.toContain('RouteDetailPanel');
    const connections = source('pages/connections/TicketWalletList.tsx');
    expect(connections).not.toContain('DetailsToggle');
    expect(connections).toContain('onShowDetail');
    expect(connections).toContain('onOpen=');
    expect(connections).not.toContain("t('connections.list.details')");
    const importDialog = source('pages/routes/shared/ImportRouteDialog.tsx');
    expect(importDialog).toContain('DetailsToggle');
    expect(importDialog).toContain("t('connections.list.details')");
    const detail = source('pages/routes/shared/RouteDetailPanel.tsx');
    expect(detail).toContain('asPanel');
    expect(detail).toContain('InspectSurface as DialogOrSide');
    expect(detail).toContain("t('routes.edit.action')");
    expect(detail).toContain("t('routes.inbound.title')");
    expect(detail).toContain("t('routes.inbound.empty')");
    expect(detail).toContain('ROUTE_LOCAL_ADDRESS_LEGEND');
    const create = source('pages/routes/shared/CreateRouteDialog.tsx');
    expect(create).toContain('localAddressCopyForTarget');
    expect(create).toContain("t('routes.endpoint.modelsLine')");
    const write = source('pages/routes/shared/WriteClientConfigDialog.tsx');
    expect(write).toContain('localEndpointKindLabel');
    expect(write).toContain("t('routes.endpoint.modelsLine')");
    expect(list).toContain("t('routes.edit.action')");
    expect(list).toContain('variant="outline"');
  });

  it('groups Claude/Codex/Grok profiles that share one source onto one card', () => {
    const page = source('pages/routes/pool/index.tsx');
    const actions = source('pages/routes/shared/use-bridge-runtime-actions.ts');
    expect(page).toContain('groupLocalBridgeProfiles');
    expect(actions).toContain('localBridgeProfilesForSource');
    const model = source('pages/routes/shared/adapter-view-model.ts');
    expect(model).toContain('One list card per upstream source');
  });

  it('opens edit and detail in the same right-hand inspect pane', () => {
    const page = source('pages/routes/pool/index.tsx');
    const inspect = source('pages/routes/shared/route-inspect.ts');
    expect(inspect).toContain("{ kind: 'edit'; profile: AdapterProfile }");
    expect(inspect).toContain("{ kind: 'detail'; profile: AdapterProfile }");
    expect(page).toContain('onShowDetail');
    expect(page).toContain("kind: 'detail'");
    expect(page).toContain('<RouteDetailPanel');
    expect(page).toContain('asPanel');
  });

  it('puts cancel on form inspect surfaces; read-only detail omits it', () => {
    const shell = source('components/layout/InspectSurface.tsx');
    expect(shell).toContain("t('common.cancel')");
    expect(shell).toContain('variant="secondary"');
    expect(shell).toContain('showCancel');
    expect(shell).toContain('{danger}');
    expect(shell).toContain('{primary}');
    expect(shell).not.toContain('footer={danger}');
    const detail = source('pages/routes/shared/RouteDetailPanel.tsx');
    expect(detail).toContain('showCancel={false}');
    expect(detail).toContain('danger={deleteButton}');
    expect(detail).toContain('variant="outline"');
    const create = source('pages/routes/shared/CreateRouteDialog.tsx');
    expect(create).not.toContain('showCancel={false}');
    const panel = source('components/layout/SideInspectPanel.tsx');
    expect(panel).toContain('PanelRightClose');
    expect(panel).toContain('flex h-10');
  });

  it('docks the recycle-bin button in the list column, left of the split', () => {
    const page = source('pages/routes/pool/index.tsx');
    const button = source('pages/connections/ConnectionTrashButton.tsx');
    const split = source('components/layout/SideSplit.tsx');
    expect(page).toContain('listFooter={trashDock}');
    expect(page).toContain('<ConnectionTrashButton');
    expect(button).not.toContain('fixed bottom-4 right-4');
    expect(split).toContain('listFooter');
    expect(split).toContain('flex shrink-0 justify-end');
  });

  it('keeps the healthy empty state informational without a second create CTA', () => {
    const page = source('pages/routes/pool/index.tsx');
    const emptyBlock = page.slice(
      page.indexOf("pageView === 'healthy_empty'"),
      page.indexOf("pageView === 'list'"),
    );
    expect(emptyBlock).toContain("t('routes.pool.page.emptyTitle')");
    expect(emptyBlock).not.toContain('actionLabel');
    expect(emptyBlock).not.toContain("t('routes.create.action')");
  });
});
