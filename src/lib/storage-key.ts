/**
 * 持久化键约定（N-15）：新键一律 `agenthub:` + kebab-case。
 * 历史 `agenthub.` + camelCase 布局键不得作为主写键；迁移读旧写新，本波仍可读旧键一轮。
 *
 * `agenthub.db` 路径、`agenthub.YYYY-MM-DD.log` 日志文件名不是 localStorage 键，不在此模块。
 */

const PREFIX = 'agenthub:';

const LAYOUT_STORAGE_KEY = {
  skillsPreviewWidth: `${PREFIX}skills-preview-width`,
  skillsMatrixLegendOpen: `${PREFIX}skills-matrix-legend-open`,
  skillsMatrixColumnWidths: `${PREFIX}skills-matrix-column-widths`,
  skillsMarketColumnWidths: `${PREFIX}skills-market-column-widths`,
  projectsPreviewWidth: `${PREFIX}projects-preview-width`,
  pluginsPreviewWidth: `${PREFIX}plugins-preview-width`,
  agentsPreviewWidth: `${PREFIX}agents-preview-width`,
  mcpColumnWidths: `${PREFIX}mcp-column-widths`,
  connectionsInspectWidth: `${PREFIX}connections-inspect-width`,
  settingsBackupsInspectWidth: `${PREFIX}settings-backups-inspect-width`,
  routesInspectWidth: `${PREFIX}routes-inspect-width`,
  routesTokensColumnWidths: `${PREFIX}routes-tokens-column-widths`,
  routesPoolColumnWidths: `${PREFIX}routes-pool-column-widths`,
  routesActivityPreviewWidth: `${PREFIX}routes-activity-preview-width`,
  routesActivityColumnWidths: `${PREFIX}routes-activity-column-widths`,
  dashboardUsageColumnWidths: `${PREFIX}dashboard-usage-column-widths`,
  chatComposerPaneHeight: `${PREFIX}chat-composer-pane-height`,
  chatBootstrap: `${PREFIX}chat-bootstrap`,
} as const;

/** 与 {@link LAYOUT_STORAGE_KEY} 一一对应的历史点号键；只用于读路径。 */
export const LegacyStorageKey = {
  skillsPreviewWidth: 'agenthub.skills.previewWidth',
  skillsMatrixLegendOpen: 'agenthub.skills.matrixLegendOpen',
  skillsMatrixColumnWidths: 'agenthub.skills.matrixColumnWidths',
  skillsMarketColumnWidths: 'agenthub.skills.marketColumnWidths',
  projectsPreviewWidth: 'agenthub.projects.previewWidth',
  pluginsPreviewWidth: 'agenthub.plugins.previewWidth',
  agentsPreviewWidth: 'agenthub.agents.previewWidth',
  mcpColumnWidths: 'agenthub.mcp.columnWidths',
  connectionsInspectWidth: 'agenthub.connections.inspectWidth',
  settingsBackupsInspectWidth: 'agenthub.settings.backupsInspectWidth',
  routesInspectWidth: 'agenthub.routes.inspectWidth',
  routesTokensColumnWidths: 'agenthub.routes.tokens.columnWidths',
  routesPoolColumnWidths: 'agenthub.routes.pool.columnWidths',
  routesActivityPreviewWidth: 'agenthub.routes.activity.previewWidth',
  routesActivityColumnWidths: 'agenthub.routes.activity.columnWidths',
  dashboardUsageColumnWidths: 'agenthub.dashboard.usageColumnWidths',
  chatComposerPaneHeight: 'agenthub.chat.composerPaneHeight',
  chatBootstrap: 'agenthub.chat.bootstrap',
} as const satisfies Record<keyof typeof LAYOUT_STORAGE_KEY, `agenthub.${string}`>;

export const StorageKey = {
  theme: `${PREFIX}theme`,
  /** Product accent id (`indigo` | `blue` | `teal` | `rose` | `amber`) */
  accent: `${PREFIX}accent`,
  language: `${PREFIX}language`,
  /** One-shot: first-launch system language already seeded into core */
  languageSystemSeeded: `${PREFIX}language-system-seeded-v1`,
  onboardingDone: `${PREFIX}onboarding-done`,
  usageGuideDismissed: `${PREFIX}usage-guide-dismissed`,
  dismissedAlertIds: `${PREFIX}dismissed-alert-ids`,
  sidebarCollapsed: `${PREFIX}sidebar-collapsed`,
  /** 点侧栏「路由」时是否自动折叠最左侧栏 */
  sidebarAutoCollapseOnRoutes: `${PREFIX}sidebar-auto-collapse-on-routes`,
  /** 侧栏是否显示「路由」入口；缺省显示（已稳定） */
  routesNavVisible: `${PREFIX}routes-nav-visible`,
  /** 侧栏是否显示「插件」入口；缺省隐藏（功能开发中） */
  pluginsNavVisible: `${PREFIX}plugins-nav-visible`,
  /** epoch ms of last successful usage collect (manual or auto) */
  usageLastCollectAt: `${PREFIX}usage-last-collect-at`,
  /** SemVer last dismissed via “稍后” on the update prompt */
  updateDismissedVersion: `${PREFIX}update-dismissed-version`,
  /** One-shot: localStorage usage interval migrated into SQLite */
  usageIntervalMigrated: `${PREFIX}usage-interval-migrated-v1`,
  connectionsTicketOrder: `${PREFIX}connections-ticket-order`,
  routesProfileOrder: `${PREFIX}routes-profile-order`,
  agentsCatalogOrder: `${PREFIX}agents-catalog-order`,
  /** Last Projects tab; used to preload that agent's list on boot. */
  projectsLastAgent: `${PREFIX}projects-last-agent`,
  /** Last workspace chosen on the project-skills tab. */
  skillsProjectWorkspace: `${PREFIX}skills-project-workspace`,
  ...LAYOUT_STORAGE_KEY,
} as const;

type LayoutStorageName = keyof typeof LAYOUT_STORAGE_KEY;

const LEGACY_BY_CANONICAL: Record<string, string> = Object.fromEntries(
  (Object.keys(LAYOUT_STORAGE_KEY) as LayoutStorageName[]).map((name) => [
    LAYOUT_STORAGE_KEY[name],
    LegacyStorageKey[name],
  ]),
);

export type StorageLike = {
  getItem(key: string): string | null;
  setItem?(key: string, value: string): void;
  removeItem?(key: string): void;
};

/** 旧键 `agenthub.foo` 对应的规范键；未登记的布局键返回 undefined。 */
export function legacyKeyFor(canonicalKey: string): string | undefined {
  return LEGACY_BY_CANONICAL[canonicalKey];
}

/**
 * 读规范键 `agenthub:foo`；没有则读旧键 `agenthub.foo` 并写回新键。
 * 主写路径不得再写旧键。
 */
export function readLegacy(storage: StorageLike, canonicalKey: string): string | null {
  try {
    const current = storage.getItem(canonicalKey);
    if (current != null) return current;
  } catch {
    /* ignore */
  }
  const legacyKey = LEGACY_BY_CANONICAL[canonicalKey];
  if (!legacyKey) return null;
  let old: string | null = null;
  try {
    old = storage.getItem(legacyKey);
  } catch {
    return null;
  }
  if (old == null) return null;
  try {
    storage.setItem?.(canonicalKey, old);
  } catch {
    /* quota / private mode */
  }
  return old;
}

/** 清掉规范键及其一轮旧键，避免删新键后回落到过期布局。 */
export function removeStorageItem(storage: StorageLike, canonicalKey: string): void {
  try {
    storage.removeItem?.(canonicalKey);
  } catch {
    /* ignore */
  }
  const legacyKey = LEGACY_BY_CANONICAL[canonicalKey];
  if (!legacyKey) return;
  try {
    storage.removeItem?.(legacyKey);
  } catch {
    /* ignore */
  }
}
