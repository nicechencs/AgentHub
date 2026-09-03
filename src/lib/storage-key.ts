/**
 * 持久化键约定（N-15）：一律 `agenthub:` + kebab-case。
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
  agentsColumnWidths: `${PREFIX}agents-column-widths`,
  mcpColumnWidths: `${PREFIX}mcp-column-widths`,
  connectionsInspectWidth: `${PREFIX}connections-inspect-width`,
  connectionsColumnWidths: `${PREFIX}connections-column-widths`,
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
  routesPoolAuthorizationOrder: `${PREFIX}routes-pool-authorization-order`,
  agentsCatalogOrder: `${PREFIX}agents-catalog-order`,
  /** Last Projects tab; used to preload that agent's list on boot. */
  projectsLastAgent: `${PREFIX}projects-last-agent`,
  /** Last workspace chosen on the project-skills tab. */
  skillsProjectWorkspace: `${PREFIX}skills-project-workspace`,
  ...LAYOUT_STORAGE_KEY,
} as const;

export type StorageLike = {
  getItem(key: string): string | null;
  setItem?(key: string, value: string): void;
  removeItem?(key: string): void;
};

/** 只读规范键。 */
export function readStorageItem(storage: StorageLike, key: string): string | null {
  try {
    return storage.getItem(key);
  } catch {
    return null;
  }
}

export function removeStorageItem(storage: StorageLike, key: string): void {
  try {
    storage.removeItem?.(key);
  } catch {
    /* ignore */
  }
}
