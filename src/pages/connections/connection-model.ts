/**
 * Connections 统一列表模型：账号池 + 供应商池 → 同一行语义。
 * 产品层：供应商已并入「API Key」（官方端点 / 自定义端点）；存储仍分表。
 */
import { formatApiConnectionLabel } from '@/lib/backend/contracts/agent-connection';
import { probeIsAdapterProjection } from '@/lib/backend/contracts/account-port';
import type { AuthHealth } from '@/lib/backend/contracts/auth-state';
import {
  CONNECTION_KIND_FILTERS,
  connectionKindLabel,
  countByConnectionKind,
  filterByConnectionKind,
  type ConnectionKind,
  type ConnectionKindFilter,
} from '@/lib/connection-kind';
import type { TranslateFn } from '@/lib/i18n';
import type { AgentKey, Provider } from '@/lib/types';
import { isLeftoverLocalRouteProvider } from '@/lib/leftover-local-route';

export type { ConnectionKind };
export type {
  ConnectionEntry,
} from '@/lib/connection-entry';
export {
  accountToEntry,
  authHealthOfAccount,
  authStatusOfAccount,
  mergeConnectionEntries,
  providerToEntry,
} from '@/lib/connection-entry';
import type { ConnectionEntry } from '@/lib/connection-entry';

export type ConnectionFilter = ConnectionKindFilter;

export const CONNECTION_FILTERS = CONNECTION_KIND_FILTERS;

export function filterConnectionEntries(
  rows: ConnectionEntry[],
  filter: ConnectionFilter,
): ConnectionEntry[] {
  return filterByConnectionKind(rows, filter, (r) => r.kind);
}

export function countByKind(rows: ConnectionEntry[]): Record<ConnectionFilter, number> {
  return countByConnectionKind(rows, (r) => r.kind);
}

export function kindBadge(kind: ConnectionKind, t?: TranslateFn): {
  label: string;
  variant: 'default' | 'info' | 'accent';
} {
  return {
    label: connectionKindLabel(kind, t),
    variant: kind === 'oauth' ? 'default' : 'info',
  };
}

export { connectionKindLabel };

export function endpointModeBadge(
  mode: 'official' | 'custom' | undefined,
  t?: TranslateFn,
): { label: string; variant: 'default' | 'info' } | null {
  if (mode === 'official') {
    return { label: t ? t('connections.list.official') : '官方', variant: 'default' };
  }
  if (mode === 'custom') {
    return { label: t ? t('connections.list.custom') : '自定义', variant: 'info' };
  }
  return null;
}

export function withProviderLatency(
  entry: ConnectionEntry,
  latencyMs: number | undefined,
  t?: TranslateFn,
): ConnectionEntry {
  if (entry.source !== 'provider' || latencyMs === undefined) return entry;
  const modeLabel = entry.endpointMode === 'custom'
    ? (t ? t('connections.list.customEndpoint') : '自定义端点')
    : (t ? t('connections.list.officialEndpoint') : '官方端点');
  const host = entry.endpointHost;
  const base = entry.isCurrent
    ? host
      ? (t
        ? t('connections.list.configuredCurrentHost', { mode: modeLabel, host })
        : `已配置 · 当前生效 · ${modeLabel} · ${host}`)
      : (t
        ? t('connections.list.configuredCurrent', { mode: modeLabel })
        : `已配置 · 当前生效 · ${modeLabel}`)
    : host
      ? (t
        ? t('connections.list.configuredIdleHost', { mode: modeLabel, host })
        : `已配置 · 未生效 · ${modeLabel} · ${host}`)
      : (t
        ? t('connections.list.configuredIdle', { mode: modeLabel })
        : `已配置 · 未生效 · ${modeLabel}`);
  return {
    ...entry,
    latencyMs,
    subtitle: t
      ? t('connections.list.latency', { base, ms: latencyMs })
      : `${base} · ${latencyMs} ms`,
  };
}

export function providerDisplayLabel(p: Provider): string {
  return formatApiConnectionLabel(p);
}

/**
 * Deleting a connection moves its AgentHub pool row to the recovery bin. It
 * never implies that the agent's local config/auth file was cleared.
 */
export function deleteConnectionDialogDescription(
  entry: Pick<ConnectionEntry, 'isCurrent'>,
  t?: TranslateFn,
): string {
  if (entry.isCurrent) {
    return t ? t('connections.delete.dialogCurrent') : '会移入回收站；本机配置不会被清除，当前连接可能仍继续生效。';
  }
  return t ? t('connections.delete.dialogOther') : '会移入回收站；不会修改本机配置文件。';
}

export function deleteConnectionToastDescription(
  entry: Pick<ConnectionEntry, 'isCurrent'>,
  t?: TranslateFn,
): string {
  if (entry.isCurrent) {
    return t ? t('connections.delete.toastCurrent') : '已移入回收站；本机配置未清除，当前连接可能仍继续生效。';
  }
  return t ? t('connections.delete.toastOther') : '已移入回收站；本机配置未修改。';
}

export type LiveAuthProbeLike = {
  agentId: AgentKey;
  kind?: string | null;
  summary?: string | null;
  hasCredentials?: boolean;
  health?: AuthHealth;
  source?: string | null;
  revision?: string | null;
  alsoPresent?: string[] | null;
  isAdapterProjection?: boolean | null;
  secretHash?: string | null;
};

export type LiveAuthImportGate = {
  enabled: boolean;
  reason: string;
};

/**
 * Import-current-login is intentionally stricter than generic auth probing:
 * only OAuth/file-auth material can be imported as an Account. API keys and
 * opaque desktop login state must not be mislabeled as OAuth.
 */
export function liveAuthImportGate(
  probe: LiveAuthProbeLike | null | undefined,
  loading: boolean,
  agentId: AgentKey,
  t?: TranslateFn,
): LiveAuthImportGate {
  if (loading) {
    return { enabled: false, reason: t ? t('connections.list.detectingLogin') : '正在查看这台电脑上的登录…' };
  }
  if (!probe) {
    return { enabled: false, reason: t ? t('connections.list.cannotConfirmLogin') : '没法确认这台电脑上的登录，暂时不能导入' };
  }
  if (probe.agentId !== agentId) {
    return { enabled: false, reason: t ? t('connections.list.loginSwitching') : '正在切换登录，暂时不能导入' };
  }
  if (probeIsAdapterProjection(probe)) {
    return {
      enabled: false,
      reason: t ? t('connections.list.liveIsLocalRoute') : '这是本机转发写进去的配置，不是一份新登录',
    };
  }

  const kind = probe.kind?.trim().toLowerCase() ?? '';
  const isFileAuth = kind === 'file-auth' || kind === 'file-auth.json';
  if ((kind === 'oauth' || isFileAuth) && probe.hasCredentials === true) {
    return { enabled: true, reason: '' };
  }
  if (kind === 'api_key' || kind === 'api-key' || kind === 'apikey') {
    return { enabled: false, reason: t ? t('connections.list.isApiKeyNotOauth') : '这台电脑上现在是 API Key，不是官方登录' };
  }
  if (kind === 'desktop-login') {
    return { enabled: false, reason: t ? t('connections.list.desktopNotImportable') : '检测到桌面版登录，但没法直接导入' };
  }
  return {
    enabled: false,
    reason: probe.summary || (t ? t('connections.list.noOauthToImport') : '没有找到可以导入的官方登录'),
  };
}

/**
 * Importing the live provider snapshot is only meaningful when the probe
 * positively identifies an API-key configuration. Keep OAuth and opaque
 * desktop sessions on the account-import path instead of labeling them as a
 * provider/API-key connection.
 */
export function liveApiKeyImportGate(
  probe: LiveAuthProbeLike | null | undefined,
  loading: boolean,
  agentId: AgentKey,
  t?: TranslateFn,
): LiveAuthImportGate {
  if (loading) {
    return { enabled: false, reason: t ? t('connections.list.detectingAuth') : '正在查看这台电脑怎么登录的…' };
  }
  if (!probe) {
    return { enabled: false, reason: t ? t('connections.list.cannotConfirmAuth') : '没法确认登录方式，暂时不能导入 API Key' };
  }
  if (probe.agentId !== agentId) {
    return { enabled: false, reason: t ? t('connections.list.authSwitching') : '正在切换登录方式，暂时不能导入 API Key' };
  }
  if (probeIsAdapterProjection(probe)) {
    return {
      enabled: false,
      reason: t ? t('connections.list.liveIsLocalRoute') : '这是本机转发写进去的配置，不是一份新登录',
    };
  }

  const kind = probe.kind?.trim().toLowerCase() ?? '';
  const isApiKey = kind === 'api_key' || kind === 'api-key' || kind === 'apikey';
  if (isApiKey && probe.hasCredentials === true) {
    return { enabled: true, reason: '' };
  }
  if (kind === 'oauth' || kind === 'file-auth' || kind === 'file-auth.json') {
    return { enabled: false, reason: t ? t('connections.list.isOauthImportLogin') : '这台电脑上是官方登录。请改用「导入授权」。' };
  }
  if (kind === 'desktop-login') {
    return { enabled: false, reason: t ? t('connections.list.desktopNoApiKey') : '这是桌面版登录，没法直接导入 API Key' };
  }
  return {
    enabled: false,
    reason: probe.summary || (t ? t('connections.list.noApiKeyToImport') : '没有找到可以导入的 API Key'),
  };
}

/** Discovery lifecycle for the connections inventory (not RoutePool). */
export type ConnectionInventoryDiscoveryState = 'idle' | 'loading' | 'ready' | 'partial' | 'error';

/** @deprecated Use {@link ConnectionInventoryDiscoveryState}. */
export type ConnectionPoolDiscoveryState = ConnectionInventoryDiscoveryState;
export type DiscoveredAuthKind = 'account' | 'provider';

/**
 * Live-auth banners must wait for a completed pool snapshot. An in-flight
 * first load still has empty rows and would otherwise look like a new login.
 */
function liveAuthProbeKind(probe?: Pick<LiveAuthProbeLike, 'kind'> | null): string {
  return probe?.kind?.trim().toLowerCase() ?? '';
}

function isOAuthLiveAuthKind(kind: string): boolean {
  return kind === 'oauth' || kind === 'file-auth' || kind === 'file-auth.json';
}

function isApiKeyLiveAuthKind(kind: string): boolean {
  return kind === 'api_key' || kind === 'api-key' || kind === 'apikey';
}

/** Which import dialog variant applies for the probed live credential kind. */
export type LiveImportDialogMode = 'login' | 'api-key';

/** Shared import dialog writes an Account (OAuth) or a provider-pool Key. */
export type LiveImportAction = 'account' | 'provider';

/**
 * The import entry point is shared; the probed auth kind decides whether it
 * reads as an OAuth login import or an API Key import. Anything unknown or
 * still loading stays on the login variant so behavior is unchanged.
 */
export function liveImportDialogMode(
  probe: LiveAuthProbeLike | null | undefined,
): LiveImportDialogMode {
  const kind = probe?.kind?.trim().toLowerCase() ?? '';
  return isApiKeyLiveAuthKind(kind) ? 'api-key' : 'login';
}

/**
 * Catalog-append agents (ZCode / WorkBuddy) split one live file into many
 * logins. Import them as accounts, not as one provider snapshot.
 * Other API Key probes still import the live provider config.
 */
export function liveImportAction(
  mode: LiveImportDialogMode,
  agentId?: AgentKey | null,
): LiveImportAction {
  if (agentId === 'zcode' || agentId === 'workbuddy') return 'account';
  return mode === 'api-key' ? 'provider' : 'account';
}

const GENERIC_LIVE_AUTH_COEXISTENCE_NOTICE =
  '这台电脑上同时有 API Key 和官方登录，它们不在同一个地方。导入只会收下现在正在用的那一份，另一份还留在本机。';

function alsoPresentKinds(probe?: Pick<LiveAuthProbeLike, 'alsoPresent'> | null): string[] {
  if (!Array.isArray(probe?.alsoPresent)) return [];
  return probe.alsoPresent
    .filter((item): item is string => typeof item === 'string')
    .map((item) => item.trim().toLowerCase())
    .filter(Boolean);
}

/**
 * Warn when a second credential family is on disk. Does not change import gates.
 */
export function liveAuthCoexistenceNotice(
  probe: LiveAuthProbeLike | null | undefined,
  agentId: AgentKey,
  t?: TranslateFn,
): string | null {
  if (!probe) return null;
  if (probeIsAdapterProjection(probe)) return null;
  const kind = liveAuthProbeKind(probe);
  const also = alsoPresentKinds(probe);
  const alsoHasOAuth = also.some(isOAuthLiveAuthKind);
  const alsoHasApiKey = also.some(isApiKeyLiveAuthKind);
  const alsoHasDesktop = also.some((item) => item === 'desktop-login');
  if (kind !== 'mixed' && !alsoHasOAuth && !alsoHasApiKey && !alsoHasDesktop) return null;

  if (
    (agentId === 'workbuddy' || agentId === 'zcode')
    && (alsoHasDesktop || kind === 'desktop-login')
  ) {
    return t
      ? t('connections.list.coexistCatalogDesktop')
      : '自定义模型和桌面套餐登录不在一起。导入只会收下自定义模型，桌面登录留在应用内。';
  }

  if (agentId === 'pi' || kind === 'mixed') {
    return t ? t('connections.list.coexistPi') : 'Pi 里同时有 API Key 和官方登录。导入会按服务商分行，不会猜一个当前账号。';
  }
  if (agentId === 'claude' && isApiKeyLiveAuthKind(kind) && alsoHasOAuth) {
    return t
      ? t('connections.list.coexistClaude')
      : '这台电脑上同时有 API Key 和官方登录。Claude 会优先用 Key（按接口计费），订阅登录会被压住。导入只会收下这份 Key。要用订阅，请先从本机配置里去掉 Key。';
  }
  if (agentId === 'grok' && isApiKeyLiveAuthKind(kind) && alsoHasOAuth) {
    return t
      ? t('connections.list.coexistGrok')
      : '这台电脑上同时有 API Key 和 grok login。写在模型上的 Key 会优先使用。导入只会收下现在正在用的那一份。';
  }
  if (agentId === 'kimi') {
    return t
      ? t('connections.list.coexistKimi')
      : '这台电脑上同时有 config.toml 里的 API Key 和官方 /login。导入只会收下现在正在用的那一份。';
  }
  if (agentId === 'codex') {
    return t
      ? t('connections.list.coexistCodex')
      : '这台电脑上同时有 API Key 和 ChatGPT 登录。导入会收下现在正在用的那一份。';
  }
  return t ? t('connections.list.coexistGeneric') : GENERIC_LIVE_AUTH_COEXISTENCE_NOTICE;
}

/**
 * Incomplete pool data must not be stamped as “already evaluated”.
 * A later successful refresh of the failed side should still be able to
 * surface a real first-time discovery.
 */
export function isLiveAuthDiscoveryDeferred(input: {
  poolState: ConnectionInventoryDiscoveryState;
  probe?: Pick<LiveAuthProbeLike, 'kind' | 'hasCredentials'> | null;
  accountsFailed?: boolean;
  providersFailed?: boolean;
}): boolean {
  if (input.poolState === 'idle' || input.poolState === 'loading' || input.poolState === 'error') {
    return true;
  }
  if (!input.probe?.hasCredentials) return false;
  const kind = liveAuthProbeKind(input.probe);
  if (isOAuthLiveAuthKind(kind)) return Boolean(input.accountsFailed);
  if (isApiKeyLiveAuthKind(kind)) return Boolean(input.accountsFailed || input.providersFailed);
  return false;
}

/**
 * Enough of a pool provider to skip leftover / adapter-projection rows when
 * deciding whether a live API Key is already imported.
 */
export type DiscoveryProviderLike = {
  id?: string | null;
  name?: string | null;
  preset?: string | null;
  configText?: string | null;
  configFormat?: string | null;
  isAdapterProjection?: boolean | null;
  alsoPresent?: string[] | null;
  secretHash?: string | null;
};

function leftoverOrProjectionProvider(provider: DiscoveryProviderLike): boolean {
  if (probeIsAdapterProjection(provider)) return true;
  return isLeftoverLocalRouteProvider({
    id: provider.id ?? '',
    name: provider.name ?? '',
    preset: provider.preset ?? '',
    configText: provider.configText ?? '',
    configFormat: provider.configFormat === 'toml' ? 'toml' : 'json',
  });
}

function normalizeSecretHash(value?: string | null): string {
  return value?.trim() ?? '';
}

function poolHasSameLiveSecret(
  probeHash: string,
  accounts: readonly { kind?: string; secretHash?: string | null }[],
  providers: readonly DiscoveryProviderLike[],
): boolean {
  if (!probeHash) return false;
  if (accounts.some((account) => normalizeSecretHash(account.secretHash) === probeHash)) {
    return true;
  }
  return providers.some((provider) => {
    if (leftoverOrProjectionProvider(provider)) return false;
    return normalizeSecretHash(provider.secretHash) === probeHash;
  });
}

export function liveAuthDiscoveryKind(input: {
  poolState: ConnectionInventoryDiscoveryState;
  probe?: Pick<LiveAuthProbeLike, 'kind' | 'hasCredentials' | 'isAdapterProjection' | 'alsoPresent' | 'secretHash'> | null;
  accounts: readonly { kind: string; secretHash?: string | null }[];
  providers: readonly DiscoveryProviderLike[];
  accountsFailed?: boolean;
  providersFailed?: boolean;
}): DiscoveredAuthKind | null {
  if (isLiveAuthDiscoveryDeferred(input)) return null;
  if (!input.probe?.hasCredentials) return null;
  if (probeIsAdapterProjection(input.probe)) return null;

  const kind = liveAuthProbeKind(input.probe);
  const hasExistingOAuth = input.accounts.some((account) => account.kind === 'oauth');
  const sameLiveSecret = poolHasSameLiveSecret(
    normalizeSecretHash(input.probe.secretHash),
    input.accounts,
    input.providers,
  );

  if (isOAuthLiveAuthKind(kind) && !hasExistingOAuth) return 'account';
  if (isApiKeyLiveAuthKind(kind) && !sameLiveSecret) return 'provider';
  return null;
}

/** A switch-preview response is only usable for the agent that requested it. */
export function isCurrentSwitchPreviewRequest(
  requestedAgentId: string,
  currentAgentId: string,
  generation: number,
  currentGeneration: number,
): boolean {
  return requestedAgentId === currentAgentId && generation === currentGeneration;
}

export function beginExclusiveBusyIds(
  current: ReadonlySet<string>,
  id: string,
): Set<string> | null {
  if (current.size > 0 || !id) return null;
  return new Set([id]);
}

export function endExclusiveBusyIds(current: ReadonlySet<string>, id: string): Set<string> {
  if (!current.has(id)) return new Set(current);
  const next = new Set(current);
  next.delete(id);
  return next;
}
