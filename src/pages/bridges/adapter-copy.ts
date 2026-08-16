import {
  CONNECTION_KIND_FILTERS,
  connectionKindFilterLabel,
  connectionKindFromAdapterProfileMode,
  connectionKindLabel,
  parseConnectionKindFilter,
  type ConnectionKind,
  type ConnectionKindFilter,
} from '@/lib/connection-kind';
import {
  BRIDGES_NAV_LABEL,
  BRIDGES_PATH,
  bridgesHrefForProfile,
  legacyBridgesRedirectTo,
} from '@/lib/bridges-path';
import type { AdapterProfileMode } from '@/lib/backend/contracts/adapter';

/**
 * Adapter page filter uses the shared connection-kind taxonomy (`all|oauth|apikey`).
 * Wire profile mode still uses backend `api|oauth`; map at edges only.
 */
export const ADAPTER_CREDENTIAL_FILTERS = CONNECTION_KIND_FILTERS.map((item) => item.value);
export type AdapterCredentialFilter = ConnectionKindFilter;

/** Legacy alias: old `?tab=api|oauth` deep links map onto the credential filter. */
export type AdapterTab = Exclude<AdapterCredentialFilter, 'all'>;

/**
 * Page filter. Missing / unknown values default to all.
 * Accepts legacy `?tab=api|oauth` and normalizes `api` → `apikey`.
 */
export function parseAdapterCredentialFilter(raw: string | null | undefined): AdapterCredentialFilter {
  return parseConnectionKindFilter(raw);
}

/** @deprecated Prefer {@link parseAdapterCredentialFilter}; unknown values now default to `all`. */
export function parseAdapterTab(raw: string | null | undefined): AdapterCredentialFilter {
  return parseAdapterCredentialFilter(raw);
}

export function adapterCredentialFilterLabel(filter: AdapterCredentialFilter): string {
  return connectionKindFilterLabel(filter);
}

export function adapterTabLabel(tab: AdapterTab | AdapterCredentialFilter): string {
  return adapterCredentialFilterLabel(tab === 'all' ? 'all' : tab);
}

export const BRIDGES_PAGE_TITLE = '本机路由';
export const BRIDGES_PAGE_DESCRIPTION = '本机协议转换 · 仅 127.0.0.1';
export const BRIDGES_PAGE_DESCRIPTION_TIP =
  '凭据在 Connections，不展示不复制。多数连接不需要本机转发。需保持托盘运行。日志不记请求正文。';
export const BRIDGES_EMPTY_TITLE = '没有本机路由';
export const BRIDGES_EMPTY_DESCRIPTION =
  '多数连接不需要本机转发。只有协议对不上时才会在这台电脑上开一层转换。若刚完成需要转发的绑定，到 Dashboard 看对应工具上的路由状态。';
export const BRIDGES_WALLET_WITHOUT_RUNTIME_TITLE = '钱包里有本机路由绑定，但找不到运行时';
export const BRIDGES_WALLET_WITHOUT_RUNTIME_DESCRIPTION = '可重试读取。不是「没有本机路由」。';
export { BRIDGES_NAV_LABEL, BRIDGES_PATH, bridgesHrefForProfile, legacyBridgesRedirectTo };

/** Unknown or missing `?profile=` stays on the list; do not toast. */
export function resolveBridgesProfileQuery(
  profileId: string | null | undefined,
  profiles: readonly { id: string }[],
): string | null {
  if (!profileId) return null;
  return profiles.some((profile) => profile.id === profileId) ? profileId : null;
}
export const BRIDGES_MUTATION_FAILURE = '本机路由操作失败';

export function adapterPageDescription(): string {
  return BRIDGES_PAGE_DESCRIPTION;
}

export function adapterTabDescription(_tab?: AdapterTab | AdapterCredentialFilter): string {
  return adapterPageDescription();
}

export function connectionKindForFilter(filter: Exclude<AdapterCredentialFilter, 'all'>): ConnectionKind {
  return filter;
}

export function connectionKindForTab(tab: AdapterTab): ConnectionKind {
  return connectionKindForFilter(tab);
}

export function adapterCredentialKindLabel(mode: AdapterProfileMode): string {
  return connectionKindLabel(connectionKindFromAdapterProfileMode(mode));
}

export function filterProfilesByMode<T extends { mode?: AdapterProfileMode | null }>(
  profiles: readonly T[],
  mode: AdapterProfileMode,
): T[] {
  return profiles.filter((profile) => profile.mode === mode);
}

export function filterProfilesByCredential<T extends { mode?: AdapterProfileMode | null }>(
  profiles: readonly T[],
  filter: AdapterCredentialFilter,
): T[] {
  if (filter === 'all') return [...profiles];
  // Page filter is `apikey`; profile wire mode is still `api`.
  const mode: AdapterProfileMode = filter === 'oauth' ? 'oauth' : 'api';
  return filterProfilesByMode(profiles, mode);
}
