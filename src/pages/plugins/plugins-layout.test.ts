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
    expect(page).not.toContain('onInstall');
    expect(page).not.toContain('installPlugin');
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
    expect(detail).not.toContain('onInstall');
    expect(detail).not.toContain('installPlugin');
  });

  it('shows enable/disable for listed Claude and Grok packs only', () => {
    const page = source('index.tsx');
    const detail = source('PluginDetailPanel.tsx');
    expect(page).toContain('enablePlugin');
    expect(page).toContain('disablePlugin');
    expect(page).not.toContain('installPlugin');
    expect(page).not.toContain('uninstallPlugin');
    expect(detail).toContain('<Switch');
    expect(detail).toContain("t('plugins.actions.toggle')");
    expect(detail).toContain("t('plugins.actions.disableHint')");
    expect(detail).toContain('canToggleListedPlugin');
    expect(detail).not.toContain('installPlugin');
    expect(detail).not.toContain('marketplaceInstall');
  });

  it('keeps the list to name, one-line description, and exception badges', () => {
    const list = source('PluginPackList.tsx');
    expect(list).toContain('plugin.description');
    expect(list).toContain("t('plugins.list.disabled')");
    expect(list).toContain("t('plugins.list.untrusted')");
    expect(list).not.toContain('plugin.marketplace');
    expect(list).not.toContain('plugin.scope');
    expect(list).not.toContain('plugin.version');
    expect(list).not.toContain("t('plugins.list.enabled')");
  });
});
