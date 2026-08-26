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
    expect(page).toContain("size=\"compact\"");
    expect(page).toContain("t('common.resizeSidePanel')");
    expect(page).toContain('pageRhythm.chrome');
    expect(page).toContain('<AgentTabStrip');
    expect(page).toContain('ListSkeleton');
    expect(page).toContain('<ErrorState');
    expect(page).toContain('<EmptyState');
    expect(page.indexOf('<AgentTabStrip')).toBeLessThan(page.indexOf('<ListSkeleton'));
    expect(page).not.toContain('onInstall');
    expect(page).not.toContain('installPlugin');
    expect(page).not.toContain('listMcpInventory');
  });

  it('opens pack details in the right-hand inspect pane', () => {
    const page = source('index.tsx');
    const detail = source('PluginDetailPanel.tsx');
    expect(page).toContain('<PluginDetailPanel');
    expect(page).toContain('inspect.open(plugin)');
    expect(detail).toContain('InspectSurface');
    expect(detail).toContain("t('plugins.detail.components')");
    expect(detail).toContain("t('plugins.detail.kindMcp')");
    expect(detail).not.toContain('onInstall');
    expect(detail).not.toContain('installPlugin');
  });
});
