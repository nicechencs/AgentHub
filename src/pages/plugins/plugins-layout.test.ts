import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const dir = path.dirname(fileURLToPath(import.meta.url));

function source(name: string): string {
  return readFileSync(path.join(dir, name), 'utf8');
}

describe('plugins layout wiring', () => {
  it('uses WorkbenchSplitPage and keeps empty/loading/error in the list column', () => {
    const page = source('index.tsx');
    expect(page).toContain('WorkbenchSplitPage');
    expect(page).toContain('PageHeader');
    expect(page).toContain("t('common.inDevelopment')");
    expect(page).toContain("t('common.resizeSidePanel')");
    expect(page).toContain('pageRhythm.chromeRow');
    expect(page).toContain('pageRhythm.chromeActions');
    expect(page).toContain('<AgentTabStrip');
    expect(page).toContain('ListSkeleton');
    expect(page).toContain('<ErrorState');
    expect(page).toContain('<EmptyState');
    expect(page.indexOf('<AgentTabStrip')).toBeLessThan(page.indexOf('<ListSkeleton'));
    expect(page).toContain('pluginEmptyCopy');
    expect(page).toContain('pluginScanFailedAgents');
    expect(page).toContain('<Notice');
    expect(page).toContain("t('plugins.empty.scanFailed'");
    expect(page).toContain('installPlugin');
    expect(page).toContain('uninstallPlugin');
    expect(page).not.toContain('listMcpInventory');
    expect(page).toContain('filterByPageVisibleAgent');
    expect(page).not.toContain('PluginSourceList');
    expect(page).not.toContain("t('plugins.sources.title')");
  });

  it('opens pack details in the right-hand inspect pane', () => {
    const page = source('index.tsx');
    const detail = source('PluginDetailPanel.tsx');
    expect(page).toContain('<PluginDetailPanel');
    expect(page).toContain('inspect.open(plugin)');
    expect(detail).toContain('InspectSurface');
    expect(detail).toContain("t('plugins.detail.components')");
    expect(detail).toContain("t('plugins.detail.kindMcp')");
    expect(detail.indexOf("t('plugins.detail.components')")).toBeLessThan(
      detail.indexOf("t('plugins.detail.version')"),
    );
    expect(detail).toContain("t('plugins.detail.requestedVersion')");
    expect(detail).toContain('pluginVersionView');
    expect(detail).toContain('onUninstall');
    expect(detail).not.toContain('installPlugin');
  });

  it('shows enable/disable for listed Claude and Grok packs only', () => {
    const page = source('index.tsx');
    const detail = source('PluginDetailPanel.tsx');
    expect(page).toContain('enablePlugin');
    expect(page).toContain('disablePlugin');
    expect(page).toContain('installPlugin');
    expect(page).toContain('uninstallPlugin');
    expect(detail).toContain('<Switch');
    expect(detail).toContain("t('plugins.actions.toggle')");
    expect(detail).toContain("t('plugins.actions.disableHint')");
    expect(detail).toContain('canToggleListedPlugin');
    expect(detail).toContain('canUninstallListedPlugin');
    expect(detail).not.toContain('marketplaceInstall');
  });

  it('opens details from a pack row that is itself role=button', () => {
    const list = source('PluginPackList.tsx');
    const row = readFileSync(
      path.resolve(dir, '../../components/shared/ListRow.tsx'),
      'utf8',
    );
    expect(list).toContain('role="button"');
    expect(list).toContain('onOpen={() => onOpen(plugin)}');
    expect(row).toContain('hit !== root');
    expect(row).toContain('isInteractiveListTarget(event.target, event.currentTarget)');
  });

  it('keeps the list to name, version, one-line description, and exception badges', () => {
    const list = source('PluginPackList.tsx');
    expect(list).toContain('plugin.description');
    expect(list).toContain('pluginVersionView');
    expect(list).toContain("t('plugins.list.disabled')");
    expect(list).toContain("t('plugins.list.untrusted')");
    expect(list).toContain("t('plugins.list.notInstalled')");
    expect(list).toContain("t('plugins.list.versionMismatch')");
    expect(list).not.toContain('plugin.marketplace');
    expect(list).not.toContain('plugin.scope');
    expect(list).not.toContain('plugin.version');
    expect(list).not.toContain("t('plugins.list.enabled')");
  });

  it('wires install and uninstall dialogs to official plugin commands', () => {
    const page = source('index.tsx');
    const install = source('PluginInstallDialog.tsx');
    const uninstall = source('PluginUninstallDialog.tsx');
    expect(page).toContain('<PluginInstallDialog');
    expect(page).toContain('<PluginUninstallDialog');
    expect(page).toContain("t('plugins.install.button')");
    expect(install).toContain('previewPluginInstall');
    expect(install).toContain('listAvailablePlugins');
    expect(install).toContain("t('plugins.install.trust')");
    expect(install).toContain('<ErrorState');
    expect(uninstall).toContain("t('plugins.uninstall.deleteData')");
    expect(uninstall).toContain('<ErrorState');
    expect(page).not.toContain('listMcpInventory');
  });
});
