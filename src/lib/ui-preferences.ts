/**
 * UiPreferencesStore — 真实 UI 本地偏好（theme / onboarding 等）。
 * 不是 backend mock；生产与 dev:mock 均可使用 localStorage。
 *
 * 持久化键约定（N-15）见 `@/lib/storage-key`：一律 `agenthub:` + kebab-case。
 */

import { readStorageItem } from '@/lib/storage-key';

export { StorageKey } from '@/lib/storage-key';

/** 新安装：点「路由」时自动折叠最左侧栏。已保存的偏好优先。 */
export const DEFAULT_SIDEBAR_AUTO_COLLAPSE_ON_ROUTES = true;
/** 新安装：侧栏显示路由入口。已保存的偏好优先。 */
export const DEFAULT_ROUTES_NAV_VISIBLE = true;
/** 新安装：侧栏不显示插件入口。已保存的偏好优先。 */
export const DEFAULT_PLUGINS_NAV_VISIBLE = false;
/** 新安装：路由二级导航不显示 Sub2API。已保存的偏好优先。 */
export const DEFAULT_SUB2API_NAV_VISIBLE = false;

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
