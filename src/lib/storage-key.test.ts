import { describe, expect, it } from 'vitest';
import { StorageKey, readStorageItem, removeStorageItem } from '@/lib/storage-key';

function memoryStorage(init?: Record<string, string>) {
  const store = new Map<string, string>(Object.entries(init ?? {}));
  return {
    store,
    getItem(key: string) {
      return store.has(key) ? store.get(key)! : null;
    },
    setItem(key: string, value: string) {
      store.set(key, String(value));
    },
    removeItem(key: string) {
      store.delete(key);
    },
  };
}

describe('StorageKey', () => {
  it('uses agenthub: kebab for every persisted key', () => {
    const values = Object.values(StorageKey);
    expect(values.length).toBeGreaterThan(0);
    for (const value of values) {
      expect(value.startsWith('agenthub:')).toBe(true);
      expect(value.slice('agenthub:'.length)).toMatch(/^[a-z0-9]+(?:-[a-z0-9]+)*$/);
    }
  });

  it('keeps layout keys on the kebab catalog', () => {
    expect({
      skillsPreviewWidth: StorageKey.skillsPreviewWidth,
      skillsMatrixLegendOpen: StorageKey.skillsMatrixLegendOpen,
      skillsMatrixColumnWidths: StorageKey.skillsMatrixColumnWidths,
      skillsMarketColumnWidths: StorageKey.skillsMarketColumnWidths,
      projectsPreviewWidth: StorageKey.projectsPreviewWidth,
      pluginsPreviewWidth: StorageKey.pluginsPreviewWidth,
      agentsPreviewWidth: StorageKey.agentsPreviewWidth,
      mcpColumnWidths: StorageKey.mcpColumnWidths,
      connectionsInspectWidth: StorageKey.connectionsInspectWidth,
      connectionsColumnWidths: StorageKey.connectionsColumnWidths,
      settingsBackupsInspectWidth: StorageKey.settingsBackupsInspectWidth,
      routesInspectWidth: StorageKey.routesInspectWidth,
      routesTokensColumnWidths: StorageKey.routesTokensColumnWidths,
      routesPoolColumnWidths: StorageKey.routesPoolColumnWidths,
      routesActivityPreviewWidth: StorageKey.routesActivityPreviewWidth,
      routesActivityColumnWidths: StorageKey.routesActivityColumnWidths,
      dashboardUsageColumnWidths: StorageKey.dashboardUsageColumnWidths,
      chatComposerPaneHeight: StorageKey.chatComposerPaneHeight,
      chatBootstrap: StorageKey.chatBootstrap,
    }).toEqual({
      skillsPreviewWidth: 'agenthub:skills-preview-width',
      skillsMatrixLegendOpen: 'agenthub:skills-matrix-legend-open',
      skillsMatrixColumnWidths: 'agenthub:skills-matrix-column-widths',
      skillsMarketColumnWidths: 'agenthub:skills-market-column-widths',
      projectsPreviewWidth: 'agenthub:projects-preview-width',
      pluginsPreviewWidth: 'agenthub:plugins-preview-width',
      agentsPreviewWidth: 'agenthub:agents-preview-width',
      mcpColumnWidths: 'agenthub:mcp-column-widths',
      connectionsInspectWidth: 'agenthub:connections-inspect-width',
      connectionsColumnWidths: 'agenthub:connections-column-widths',
      settingsBackupsInspectWidth: 'agenthub:settings-backups-inspect-width',
      routesInspectWidth: 'agenthub:routes-inspect-width',
      routesTokensColumnWidths: 'agenthub:routes-tokens-column-widths',
      routesPoolColumnWidths: 'agenthub:routes-pool-column-widths',
      routesActivityPreviewWidth: 'agenthub:routes-activity-preview-width',
      routesActivityColumnWidths: 'agenthub:routes-activity-column-widths',
      dashboardUsageColumnWidths: 'agenthub:dashboard-usage-column-widths',
      chatComposerPaneHeight: 'agenthub:chat-composer-pane-height',
      chatBootstrap: 'agenthub:chat-bootstrap',
    });
  });
});

describe('readStorageItem', () => {
  it('reads only the canonical key', () => {
    const storage = memoryStorage({
      [StorageKey.skillsPreviewWidth]: '480',
    });
    expect(readStorageItem(storage, StorageKey.skillsPreviewWidth)).toBe('480');
    expect(readStorageItem(storage, StorageKey.theme)).toBeNull();
    expect(readStorageItem(storage, StorageKey.routesInspectWidth)).toBeNull();
  });
});

describe('removeStorageItem', () => {
  it('clears the canonical key', () => {
    const storage = memoryStorage({
      [StorageKey.chatBootstrap]: 'new',
    });
    removeStorageItem(storage, StorageKey.chatBootstrap);
    expect(storage.getItem(StorageKey.chatBootstrap)).toBeNull();
  });
});
