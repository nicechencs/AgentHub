import { describe, expect, it } from 'vitest';
import {
  LegacyStorageKey,
  StorageKey,
  legacyKeyFor,
  readLegacy,
  removeStorageItem,
} from '@/lib/storage-key';

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

describe('StorageKey layout migration', () => {
  it('maps every leftover dotted layout key onto agenthub: kebab', () => {
    expect({
      skillsPreviewWidth: [LegacyStorageKey.skillsPreviewWidth, StorageKey.skillsPreviewWidth],
      skillsMatrixLegendOpen: [
        LegacyStorageKey.skillsMatrixLegendOpen,
        StorageKey.skillsMatrixLegendOpen,
      ],
      skillsMatrixColumnWidths: [
        LegacyStorageKey.skillsMatrixColumnWidths,
        StorageKey.skillsMatrixColumnWidths,
      ],
      skillsMarketColumnWidths: [
        LegacyStorageKey.skillsMarketColumnWidths,
        StorageKey.skillsMarketColumnWidths,
      ],
      projectsPreviewWidth: [LegacyStorageKey.projectsPreviewWidth, StorageKey.projectsPreviewWidth],
      pluginsPreviewWidth: [LegacyStorageKey.pluginsPreviewWidth, StorageKey.pluginsPreviewWidth],
      agentsPreviewWidth: [LegacyStorageKey.agentsPreviewWidth, StorageKey.agentsPreviewWidth],
      mcpColumnWidths: [LegacyStorageKey.mcpColumnWidths, StorageKey.mcpColumnWidths],
      connectionsInspectWidth: [
        LegacyStorageKey.connectionsInspectWidth,
        StorageKey.connectionsInspectWidth,
      ],
      settingsBackupsInspectWidth: [
        LegacyStorageKey.settingsBackupsInspectWidth,
        StorageKey.settingsBackupsInspectWidth,
      ],
      routesInspectWidth: [LegacyStorageKey.routesInspectWidth, StorageKey.routesInspectWidth],
      routesTokensColumnWidths: [
        LegacyStorageKey.routesTokensColumnWidths,
        StorageKey.routesTokensColumnWidths,
      ],
      routesPoolColumnWidths: [
        LegacyStorageKey.routesPoolColumnWidths,
        StorageKey.routesPoolColumnWidths,
      ],
      routesActivityPreviewWidth: [
        LegacyStorageKey.routesActivityPreviewWidth,
        StorageKey.routesActivityPreviewWidth,
      ],
      routesActivityColumnWidths: [
        LegacyStorageKey.routesActivityColumnWidths,
        StorageKey.routesActivityColumnWidths,
      ],
      dashboardUsageColumnWidths: [
        LegacyStorageKey.dashboardUsageColumnWidths,
        StorageKey.dashboardUsageColumnWidths,
      ],
      chatComposerPaneHeight: [
        LegacyStorageKey.chatComposerPaneHeight,
        StorageKey.chatComposerPaneHeight,
      ],
      chatBootstrap: [LegacyStorageKey.chatBootstrap, StorageKey.chatBootstrap],
    }).toEqual({
      skillsPreviewWidth: ['agenthub.skills.previewWidth', 'agenthub:skills-preview-width'],
      skillsMatrixLegendOpen: [
        'agenthub.skills.matrixLegendOpen',
        'agenthub:skills-matrix-legend-open',
      ],
      skillsMatrixColumnWidths: [
        'agenthub.skills.matrixColumnWidths',
        'agenthub:skills-matrix-column-widths',
      ],
      skillsMarketColumnWidths: [
        'agenthub.skills.marketColumnWidths',
        'agenthub:skills-market-column-widths',
      ],
      projectsPreviewWidth: ['agenthub.projects.previewWidth', 'agenthub:projects-preview-width'],
      pluginsPreviewWidth: ['agenthub.plugins.previewWidth', 'agenthub:plugins-preview-width'],
      agentsPreviewWidth: ['agenthub.agents.previewWidth', 'agenthub:agents-preview-width'],
      mcpColumnWidths: ['agenthub.mcp.columnWidths', 'agenthub:mcp-column-widths'],
      connectionsInspectWidth: [
        'agenthub.connections.inspectWidth',
        'agenthub:connections-inspect-width',
      ],
      settingsBackupsInspectWidth: [
        'agenthub.settings.backupsInspectWidth',
        'agenthub:settings-backups-inspect-width',
      ],
      routesInspectWidth: ['agenthub.routes.inspectWidth', 'agenthub:routes-inspect-width'],
      routesTokensColumnWidths: [
        'agenthub.routes.tokens.columnWidths',
        'agenthub:routes-tokens-column-widths',
      ],
      routesPoolColumnWidths: [
        'agenthub.routes.pool.columnWidths',
        'agenthub:routes-pool-column-widths',
      ],
      routesActivityPreviewWidth: [
        'agenthub.routes.activity.previewWidth',
        'agenthub:routes-activity-preview-width',
      ],
      routesActivityColumnWidths: [
        'agenthub.routes.activity.columnWidths',
        'agenthub:routes-activity-column-widths',
      ],
      dashboardUsageColumnWidths: [
        'agenthub.dashboard.usageColumnWidths',
        'agenthub:dashboard-usage-column-widths',
      ],
      chatComposerPaneHeight: [
        'agenthub.chat.composerPaneHeight',
        'agenthub:chat-composer-pane-height',
      ],
      chatBootstrap: ['agenthub.chat.bootstrap', 'agenthub:chat-bootstrap'],
    });
  });
});

describe('readLegacy', () => {
  it('prefers the canonical key and does not copy when it already exists', () => {
    const storage = memoryStorage({
      [StorageKey.skillsPreviewWidth]: '480',
      [LegacyStorageKey.skillsPreviewWidth]: '300',
    });
    expect(readLegacy(storage, StorageKey.skillsPreviewWidth)).toBe('480');
    expect(storage.getItem(LegacyStorageKey.skillsPreviewWidth)).toBe('300');
  });

  it('reads agenthub.foo and write-through to agenthub:foo', () => {
    const storage = memoryStorage({
      [LegacyStorageKey.mcpColumnWidths]: '{"name":240}',
    });
    expect(readLegacy(storage, StorageKey.mcpColumnWidths)).toBe('{"name":240}');
    expect(storage.getItem(StorageKey.mcpColumnWidths)).toBe('{"name":240}');
    expect(legacyKeyFor(StorageKey.mcpColumnWidths)).toBe('agenthub.mcp.columnWidths');
  });

  it('returns null when neither key exists, or the canonical key has no legacy twin', () => {
    const storage = memoryStorage();
    expect(readLegacy(storage, StorageKey.theme)).toBeNull();
    expect(legacyKeyFor(StorageKey.theme)).toBeUndefined();
    expect(readLegacy(storage, StorageKey.routesInspectWidth)).toBeNull();
  });
});

describe('removeStorageItem', () => {
  it('clears both the canonical key and its leftover dotted twin', () => {
    const storage = memoryStorage({
      [StorageKey.chatBootstrap]: 'new',
      [LegacyStorageKey.chatBootstrap]: 'old',
    });
    removeStorageItem(storage, StorageKey.chatBootstrap);
    expect(storage.getItem(StorageKey.chatBootstrap)).toBeNull();
    expect(storage.getItem(LegacyStorageKey.chatBootstrap)).toBeNull();
  });
});
