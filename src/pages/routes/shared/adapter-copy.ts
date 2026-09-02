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
  ROUTES_NAV_LABEL,
  ROUTES_PATH,
  routesHrefForProfile,
  legacyBridgesRedirectTo,
} from '@/lib/routes-path';
import type { AdapterProfileMode } from '@/lib/backend/contracts/adapter';
import type { TranslateFn } from '@/lib/i18n';

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

export const ROUTES_PAGE_TITLE = '路由';
export const ROUTES_PAGE_DESCRIPTION = '本机转发 · 仅 127.0.0.1 · 含端口';
export const ROUTES_PAGE_DESCRIPTION_TIP =
  '登录信息在连接页，不展示不复制。客户端填本机地址：/v1/messages Claude 对话、/v1/responses Codex / Grok 对话、/v1/chat/completions Kimi 等补全、GET /models 模型名单。需保持托盘运行。';
export const BRIDGES_EMPTY_TITLE = '没有本机路由';
export const BRIDGES_EMPTY_DESCRIPTION =
  '多数连接不需要这一步。需要时在连接页把登录接到工具。';
export const BRIDGES_WALLET_WITHOUT_RUNTIME_TITLE = '登录列表里有本机路由，但找不到正在运行的转发';
export const BRIDGES_WALLET_WITHOUT_RUNTIME_DESCRIPTION = '点重试。不是「没有本机路由」。';
export { ROUTES_NAV_LABEL, ROUTES_PATH, routesHrefForProfile, legacyBridgesRedirectTo };

/** Unknown or missing `?profile=` stays on the auth-pool workbench; do not toast. */
export function resolveBridgesProfileQuery(
  profileId: string | null | undefined,
  profiles: readonly { id: string }[],
): string | null {
  if (!profileId) return null;
  return profiles.some((profile) => profile.id === profileId) ? profileId : null;
}
export const BRIDGES_MUTATION_FAILURE = '本机路由操作失败，可点重试。';

export function adapterPageDescription(): string {
  return ROUTES_PAGE_DESCRIPTION;
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

export function adapterCredentialKindLabel(mode: AdapterProfileMode, t?: TranslateFn): string {
  return connectionKindLabel(connectionKindFromAdapterProfileMode(mode), t);
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
