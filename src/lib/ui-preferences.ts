/**
 * UiPreferencesStore — 真实 UI 本地偏好（theme / onboarding 等）。
 * 不是 backend mock；生产与 dev:mock 均可使用 localStorage。
 *
 * 持久化键约定（N-15）见 `@/lib/storage-key`：一律 `agenthub:` + kebab-case。
 */

import { readStorageItem, StorageKey } from '@/lib/storage-key';

export { StorageKey } from '@/lib/storage-key';

/** 新安装：点「路由」时自动折叠最左侧栏。已保存的偏好优先。 */
export const DEFAULT_SIDEBAR_AUTO_COLLAPSE_ON_ROUTES = true;
/** 新安装：侧栏显示路由入口。已保存的偏好优先。 */
export const DEFAULT_ROUTES_NAV_VISIBLE = true;
/** 新安装：侧栏不显示插件入口。已保存的偏好优先。 */
export const DEFAULT_PLUGINS_NAV_VISIBLE = false;
/** 新安装：侧栏不显示 Sub2API。已保存的偏好优先。 */
export const DEFAULT_SUB2API_NAV_VISIBLE = false;
/** 新安装：侧栏显示技能入口。已保存的偏好优先。 */
export const DEFAULT_SKILLS_NAV_VISIBLE = true;
/** 新安装：侧栏显示 MCP 入口。已保存的偏好优先。 */
export const DEFAULT_MCP_NAV_VISIBLE = true;
/** 新安装：侧栏显示项目入口。已保存的偏好优先。 */
export const DEFAULT_PROJECTS_NAV_VISIBLE = true;
/** 新安装：侧栏显示连接入口。已保存的偏好优先。 */
export const DEFAULT_CONNECTIONS_NAV_VISIBLE = true;

/**
 * 可在设置里开关的侧栏入口，顺序与侧栏一致（工作区再管理）。
 * 对话、Agent、总览、设置始终显示。
 */
export const OPTIONAL_NAV_IDS = [
  'skills',
  'mcp',
  'projects',
  'plugins',
  'connections',
  'sub2api',
  'routes',
] as const;

export type OptionalNavId = (typeof OPTIONAL_NAV_IDS)[number];

export type NavVisibility = Record<OptionalNavId, boolean>;

export const DEFAULT_NAV_VISIBILITY: NavVisibility = {
  skills: DEFAULT_SKILLS_NAV_VISIBLE,
  mcp: DEFAULT_MCP_NAV_VISIBLE,
  projects: DEFAULT_PROJECTS_NAV_VISIBLE,
  plugins: DEFAULT_PLUGINS_NAV_VISIBLE,
  connections: DEFAULT_CONNECTIONS_NAV_VISIBLE,
  sub2api: DEFAULT_SUB2API_NAV_VISIBLE,
  routes: DEFAULT_ROUTES_NAV_VISIBLE,
};

export const OPTIONAL_NAV_STORAGE_KEY: Record<OptionalNavId, string> = {
  skills: StorageKey.skillsNavVisible,
  mcp: StorageKey.mcpNavVisible,
  projects: StorageKey.projectsNavVisible,
  plugins: StorageKey.pluginsNavVisible,
  connections: StorageKey.connectionsNavVisible,
  sub2api: StorageKey.sub2apiNavVisible,
  routes: StorageKey.routesNavVisible,
};

export const ALL_NAV_VISIBLE: NavVisibility = {
  skills: true,
  mcp: true,
  projects: true,
  plugins: true,
  connections: true,
  sub2api: true,
  routes: true,
};

export const ALL_NAV_HIDDEN: NavVisibility = {
  skills: false,
  mcp: false,
  projects: false,
  plugins: false,
  connections: false,
  sub2api: false,
  routes: false,
};

export function navVisibilityWith(
  overrides: Partial<NavVisibility>,
  base: NavVisibility = ALL_NAV_VISIBLE,
): NavVisibility {
  return { ...base, ...overrides };
}

export function loadJson<T>(key: string, fallback: T): T {
  try {
    const raw = readStorageItem(localStorage, key);
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
    return readStorageItem(localStorage, key) ?? fallback;
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
    const raw = readStorageItem(localStorage, key);
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
