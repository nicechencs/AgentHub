import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { agentHasCatalogUpdate, sidebarInstallStats } from './sidebar-stats';

const dir = path.dirname(fileURLToPath(import.meta.url));

function sidebarSource(): string {
  return readFileSync(path.join(dir, 'Sidebar.tsx'), 'utf8');
}

const catalog = [{ id: 'claude' }, { id: 'codex' }, { id: 'dsh' }, { id: 'kimi' }];

describe('sidebarInstallStats', () => {
  it('omits a hidden installed agent from the numerator and the dot list', () => {
    const stats = sidebarInstallStats(catalog, [
      { agentId: 'claude', installed: true, hidden: true },
      { agentId: 'codex', installed: true },
      { agentId: 'dsh', installed: true },
      { agentId: 'kimi', installed: true },
    ]);
    expect(stats.installedCount).toBe(3);
    expect(stats.visibleTotal).toBe(3);
    expect(stats.orderedInstalledMetas.map((row) => row.id)).toEqual(['codex', 'dsh', 'kimi']);
    expect([...stats.hiddenIds]).toEqual(['claude']);
  });

  it('counts an uninstalled visible agent only in visibleTotal', () => {
    const stats = sidebarInstallStats(catalog, [
      { agentId: 'claude', installed: true },
      { agentId: 'codex', installed: false },
    ]);
    expect(stats.installedCount).toBe(1);
    expect(stats.visibleTotal).toBe(4);
    expect(stats.orderedInstalledMetas.map((row) => row.id)).toEqual(['claude']);
  });

  it('applies stored catalog order to installed dots', () => {
    const stats = sidebarInstallStats(
      catalog,
      [
        { agentId: 'claude', installed: true },
        { agentId: 'dsh', installed: true },
      ],
      ['dsh', 'claude'],
    );
    expect(stats.orderedInstalledMetas.map((row) => row.id)).toEqual(['dsh', 'claude']);
  });
});

describe('agentHasCatalogUpdate', () => {
  it('is false without latestVersion or when versions match', () => {
    expect(agentHasCatalogUpdate({ installed: true, version: '1.0.0' })).toBe(false);
    expect(
      agentHasCatalogUpdate({
        installed: true,
        version: '1.0.0',
        latestVersion: '1.0.0',
      }),
    ).toBe(false);
  });

  it('is true when installed versions differ', () => {
    expect(
      agentHasCatalogUpdate({
        installed: true,
        version: '1.0.0',
        latestVersion: '1.1.0',
      }),
    ).toBe(true);
  });

  it('is false when not installed even if versions differ', () => {
    expect(
      agentHasCatalogUpdate({
        installed: false,
        version: '1.0.0',
        latestVersion: '1.1.0',
      }),
    ).toBe(false);
  });
});

describe('sidebar layout vs stats', () => {
  it('composes the stats model and does not re-filter hidden rows', () => {
    const src = sidebarSource();
    expect(src).toContain('sidebarInstallStats');
    expect(src).toContain('agentHasCatalogUpdate');
    expect(src).not.toContain('a.hidden');
    expect(src).not.toMatch(/filter\(\(a\) => a\.hidden\)/);
  });
});
