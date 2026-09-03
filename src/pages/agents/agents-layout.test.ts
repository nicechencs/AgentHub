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
    expect(source('App.tsx')).toContain("pathname === '/agents'");
    expect(page).toContain('PageHeader');
    expect(page).toContain("t('common.resizeSidePanel')");
    expect(page).toContain('<AgentDetailPanel');
    expect(page).toContain('inspect.open(a.agentId)');
    expect(page).toContain('onSelect={() => inspect.open(a.agentId)}');
    expect(page).toContain('if (!liveIds.includes(inspect.target)) inspect.close()');
    expect(page).not.toContain('a.installed ? () => inspect.open(a.agentId) : undefined');
    expect(page).not.toContain('if (!inspectAgent?.installed) inspect.close()');
    expect(page).toContain('ListSkeleton');
    expect(page).toContain('<ErrorState');
    expect(page).toContain('<EmptyState');
    expect(detail).toContain('InspectSurface');
    expect(detail).toContain("t('agents.detail.installLocations')");
    expect(detail).toContain("t('agents.detail.endpointTypes')");
    expect(detail).toContain('EndpointTypesField');
    expect(detail).toContain('RouteEndpointTypeText');
    expect(detail).toContain('agentConversationEndpoints');
    expect(detail).toContain('installChannelKindLabel');
    expect(detail).toContain('installLocationSourceLabel');
    expect(detail).toContain('displayAgentConfigDir');
    expect(detail).toContain("from '@/components/shared/OpenDirButton'");
    expect(detail).toContain("t('agents.card.openInstallDir')");
    expect(detail).toContain('flex items-center justify-between gap-2');
    expect(detail).toContain('<OpenDirButton');
    expect(detail).toContain('ChannelUninstallButton');
    expect(detail).toContain("title={t('agents.card.openConfigDirTitle')}");
    expect(detail).toContain("title={t('agents.card.openInstallDir')}");
    expect(card).toContain('agentListDetailsHint');
    expect(card).toContain('agentLaunchTargets');
    expect(card).toContain("t('agents.card.startCli')");
    expect(card).toContain("t('agents.card.startApp')");
    expect(detail).toContain("t('agents.card.uninstallProgram')");
    expect(detail).toContain("t('agents.card.uninstallConfig')");
    expect(detail).toContain('listAgentInstalls');
    expect(detail).toContain('missingCatalogChannels');
    expect(detail).toContain('MissingChannelRow');
    expect(detail).toContain('CopyableCommand');
    expect(detail).toContain('<Badge variant="success">{t(\'agents.card.spawnCopy\')}</Badge>');
    expect(detail).toContain('CopyableChannelName');
    expect(detail).toContain('copyableChannelCommand');
    expect(detail).toContain('agents.card.copyCommand');
    expect(detail).toContain('AgentInstallButton');
    expect(detail).toContain('ChannelUpgradeButton');
    expect(detail).toContain('installAgentDetailed');
    expect(source('pages/agents/AgentInstallButton.tsx')).toContain('agents.card.install');
    expect(card).toContain('<AgentInstallButton');
    expect(detail).toContain('upgradeAgentDetailed');
    expect(detail).toContain('openPathInFileManager');
    expect(detail).toContain('openAgentConfig');
    expect(page).toContain('runtimes={runtimes}');
    expect(page).toContain('EnvSoftwareList');
    expect(page).not.toContain('<EnvStatusBar');
    expect(page).toContain('onAction={(runtime, intent)');
    expect(page).toContain("intent: 'install'");
    expect(card).toContain('onSelect?:');
    expect(card).toContain('selected?:');
    expect(card).toContain('data-agent-name');
    expect(card).not.toContain('onOpen={onSelect}');
    expect(card).toContain('TableRow');
    expect(card).not.toContain('ListRowBody');
    expect(card).not.toContain('LIST_ROW_PAD');
    expect(card).toContain('size="sm"');
    expect(card).not.toContain('min-h-20');
    expect(card).toContain('uniqueInstallVersions');
    expect(card).toContain('programInstalls');
    expect(card).not.toContain("t('agents.card.openInstallDir')");
    expect(card).not.toContain("t('agents.card.uninstallProgram')");
    expect(card).not.toContain("t('agents.detail.endpointTypes')");
    expect(card).not.toContain('formatAgentConversationEndpoints');
    expect(card).not.toContain('<Hint label={inst.location}');
    expect(page).toContain('TableShell');
    expect(page).not.toContain('pageRhythm.stackDense');
  });

  it('lets the uninstall type-to-confirm name copy with one click', () => {
    const dialogs = source('pages/agents/AgentCardDialogs.tsx');
    expect(dialogs).toContain("t('agents.dialog.typeToConfirm', { name: agentName })");
    expect(dialogs).toContain('navigator.clipboard.writeText(agentName)');
    expect(dialogs).toContain("toast({ title: t('common.copied'), variant: 'success' })");
    expect(dialogs).toContain("aria-label={t('common.copy')}");
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
    expect(page).toContain('resolveAgentMeta(a.agentId).color');
    expect(page).toContain('color={resolveAgentMeta(a.agentId).color}');
    const card = source('pages/agents/agent-card.tsx');
    expect(card).toContain('sortHandle?:');
    expect(card).toContain('{sortHandle}');
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
    const keys = source('lib/storage-key.ts');
    expect(keys).toContain('agentsCatalogOrder');
    expect(keys).toContain('connectionsTicketOrder');
    expect(keys).toContain('routesProfileOrder');
  });
});
