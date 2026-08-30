/**
 * UiPreferencesStore — 真实 UI 本地偏好（theme / onboarding 等）。
 * 不是 backend mock；生产与 dev:mock 均可使用 localStorage。
 */

const PREFIX = 'agenthub:';

export const StorageKey = {
  theme: `${PREFIX}theme`,
  language: `${PREFIX}language`,
  /** One-shot: first-launch system language already seeded into core */
  languageSystemSeeded: `${PREFIX}language-system-seeded-v1`,
  onboardingDone: `${PREFIX}onboarding-done`,
  usageGuideDismissed: `${PREFIX}usage-guide-dismissed`,
  dismissedAlertIds: `${PREFIX}dismissed-alert-ids`,
  sidebarCollapsed: `${PREFIX}sidebar-collapsed`,
  /** 侧栏是否显示「路由」入口；缺省为可见 */
  routesNavVisible: `${PREFIX}routes-nav-visible`,
  /** 侧栏是否显示「插件」入口；缺省为可见 */
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
} as const;

export function loadJson<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key);
    if (raw == null) return fallback;
    return JSON.parse(raw) as T;
  } catch {
    return fallback;
  }
}

export function saveJson(key: string, value: unknown): void {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // 隐私模式 / 配额满时忽略
  }
}

export function loadString(key: string, fallback: string): string {
  try {
    return localStorage.getItem(key) ?? fallback;
  } catch {
    return fallback;
  }
}

export function saveString(key: string, value: string): void {
  try {
    localStorage.setItem(key, value);
  } catch {
    // ignore
  }
}

export function loadBool(key: string, fallback = false): boolean {
  try {
    const raw = localStorage.getItem(key);
    if (raw == null) return fallback;
    return raw === '1' || raw === 'true';
  } catch {
    return fallback;
  }
}

export function saveBool(key: string, value: boolean): void {
  try {
    localStorage.setItem(key, value ? '1' : '0');
  } catch {
    // ignore
  }
}
