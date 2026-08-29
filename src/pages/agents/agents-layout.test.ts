import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const dir = path.dirname(fileURLToPath(import.meta.url));
const srcRoot = path.resolve(dir, '../..');

function source(rel: string): string {
  return readFileSync(path.join(srcRoot, rel), 'utf8');
}

describe('agents layout wiring', () => {
  it('uses WorkbenchSplitPage and opens installed-agent details in the right pane', () => {
    const page = source('pages/agents/index.tsx');
    const detail = source('pages/agents/AgentDetailPanel.tsx');
    const card = source('pages/agents/agent-card.tsx');
    expect(page).toContain('WorkbenchSplitPage');
    expect(page).toContain("size=\"compact\"");
    expect(page).toContain("t('common.resizeSidePanel')");
    expect(page).toContain('<AgentDetailPanel');
    expect(page).toContain('inspect.open(a.agentId)');
    expect(page).toContain('a.installed ? () => inspect.open(a.agentId) : undefined');
    expect(page).toContain('if (!inspectAgent?.installed) inspect.close()');
    expect(page).toContain('ListSkeleton');
    expect(page).toContain('<ErrorState');
    expect(page).toContain('<EmptyState');
    expect(detail).toContain('InspectSurface');
    expect(detail).toContain("t('agents.detail.installLocations')");
    expect(detail).toContain("t('agents.detail.endpointTypes')");
    expect(detail).toContain('installChannelDisplayLabel');
    expect(detail).toContain('displayAgentConfigDir');
    expect(detail).toContain("t('agents.detail.openFolder')");
    expect(detail).toContain('flex items-center justify-between gap-2');
    expect(detail).toContain('<OpenDirButton');
    expect(detail).toContain("title={t('agents.card.openConfigDirTitle')}");
    expect(detail).toContain("title={t('agents.card.openInstallDir')}");
    expect(card).toContain("t('agents.card.seeDetails')");
    expect(detail).toContain("t('agents.card.uninstallProgram')");
    expect(detail).toContain("t('agents.card.uninstallConfig')");
    expect(detail).toContain('listAgentInstalls');
    expect(detail).toContain('openPathInFileManager');
    expect(detail).toContain('openAgentConfig');
    expect(card).toContain('onSelect?:');
    expect(card).toContain('selected?:');
    expect(card).toContain('onOpen={onSelect}');
    expect(card).toContain('ListRowBody');
    expect(card).toContain('LIST_ROW_PAD');
    expect(card).toContain('size="sm"');
    expect(card).not.toContain('min-h-20');
    expect(card).toContain("t('agents.card.seeDetails')");
    expect(card).toContain('uniqueInstallVersions');
    expect(card).not.toContain("t('agents.card.openInstallDir')");
    expect(card).not.toContain("t('agents.card.uninstallProgram')");
    expect(card).not.toContain("t('agents.detail.endpointTypes')");
    expect(card).not.toContain('formatAgentConversationEndpoints');
    expect(card).not.toContain('<Hint label={inst.location}');
    expect(page).toContain('pageRhythm.stackDense');
  });

  it('drag-reorders Agent cards and remembers the catalog order', () => {
    const page = source('pages/agents/index.tsx');
    expect(page).toContain('SortHandle');
    expect(page).toContain('useSortableDrag');
    expect(page).toContain('onDragStartId');
    expect(page).toContain('useStoredIdOrder');
    expect(page).toContain('StorageKey.agentsCatalogOrder');
    expect(page).toContain('applyStoredAgentOrder');
    expect(page).toContain('sortHandle=');
    const card = source('pages/agents/agent-card.tsx');
    expect(card).toContain('sortHandle?:');
    expect(card).toContain('leading={sortHandle}');
  });

  it('applies the same remembered order on sidebar, dashboard, and installed lists', () => {
    const sidebar = source('components/layout/Sidebar.tsx');
    expect(sidebar).toContain('StorageKey.agentsCatalogOrder');
    const sidebarStats = source('components/layout/sidebar-stats.ts');
    expect(sidebarStats).toContain('applyStoredAgentOrder');
    const overview = source('pages/dashboard/AgentOverview.tsx');
    expect(overview).toContain('StorageKey.agentsCatalogOrder');
    const dashboard = source('pages/dashboard/index.tsx');
    expect(dashboard).toContain('installedAgents');
    expect(dashboard).toContain('omittedIds');
    const hook = source('lib/hooks/useInstalledAgents.ts');
    expect(hook).toContain('StorageKey.agentsCatalogOrder');
    const prefs = source('lib/ui-preferences.ts');
    expect(prefs).toContain('agentsCatalogOrder');
    expect(prefs).toContain('connectionsTicketOrder');
    expect(prefs).toContain('routesProfileOrder');
  });
});
