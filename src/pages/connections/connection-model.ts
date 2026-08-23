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
import type { AgentId, Provider } from '@/lib/types';

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
  agentId: AgentId;
  kind?: string | null;
  summary?: string | null;
  hasCredentials?: boolean;
  health?: AuthHealth;
  source?: string | null;
  revision?: string | null;
  alsoPresent?: string[] | null;
  isAdapterProjection?: boolean | null;
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
  agentId: AgentId,
  t?: TranslateFn,
): LiveAuthImportGate {
  if (loading) {
    return { enabled: false, reason: t ? t('connections.list.detectingLogin') : '正在检测本机登录态…' };
  }
  if (!probe) {
    return { enabled: false, reason: t ? t('connections.list.cannotConfirmLogin') : '无法确认本机登录态，已禁用导入' };
  }
  if (probe.agentId !== agentId) {
    return { enabled: false, reason: t ? t('connections.list.loginSwitching') : '本机登录态正在切换，已禁用导入' };
  }
  if (probeIsAdapterProjection(probe)) {
    return {
      enabled: false,
      reason: t ? t('connections.list.liveIsLocalRoute') : '当前是本机路由写进去的配置，不是一份新登录',
    };
  }

  const kind = probe.kind?.trim().toLowerCase() ?? '';
  const isFileAuth = kind === 'file-auth' || kind === 'file-auth.json';
  if ((kind === 'oauth' || isFileAuth) && probe.hasCredentials === true) {
    return { enabled: true, reason: '' };
  }
  if (kind === 'api_key' || kind === 'api-key' || kind === 'apikey') {
    return { enabled: false, reason: t ? t('connections.list.isApiKeyNotOauth') : '当前本机配置为 API Key，不是 OAuth 登录态' };
  }
  if (kind === 'desktop-login') {
    return { enabled: false, reason: t ? t('connections.list.desktopNotImportable') : '检测到桌面登录，但该登录态不可直接导入' };
  }
  return {
    enabled: false,
    reason: probe.summary || (t ? t('connections.list.noOauthToImport') : '未检测到可导入的 OAuth 登录态'),
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
  agentId: AgentId,
  t?: TranslateFn,
): LiveAuthImportGate {
  if (loading) {
    return { enabled: false, reason: t ? t('connections.list.detectingAuth') : '正在检测本机认证方式…' };
  }
  if (!probe) {
    return { enabled: false, reason: t ? t('connections.list.cannotConfirmAuth') : '无法确认本机认证方式，已禁用 API Key 导入' };
  }
  if (probe.agentId !== agentId) {
    return { enabled: false, reason: t ? t('connections.list.authSwitching') : '本机认证方式正在切换，已禁用 API Key 导入' };
  }
  if (probeIsAdapterProjection(probe)) {
    return {
      enabled: false,
      reason: t ? t('connections.list.liveIsLocalRoute') : '当前是本机路由写进去的配置，不是一份新登录',
    };
  }

  const kind = probe.kind?.trim().toLowerCase() ?? '';
  const isApiKey = kind === 'api_key' || kind === 'api-key' || kind === 'apikey';
  if (isApiKey && probe.hasCredentials === true) {
    return { enabled: true, reason: '' };
  }
  if (kind === 'oauth' || kind === 'file-auth' || kind === 'file-auth.json') {
    return { enabled: false, reason: t ? t('connections.list.isOauthImportLogin') : '当前本机为 OAuth 登录态，请导入当前授权' };
  }
  if (kind === 'desktop-login') {
    return { enabled: false, reason: t ? t('connections.list.desktopNoApiKey') : '当前为桌面登录态，无法直接导入 API Key' };
  }
  return {
    enabled: false,
    reason: probe.summary || (t ? t('connections.list.noApiKeyToImport') : '未检测到可导入的 API Key'),
  };
}

export type ConnectionPoolDiscoveryState = 'idle' | 'loading' | 'ready' | 'partial' | 'error';
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

const GENERIC_LIVE_AUTH_COEXISTENCE_NOTICE =
  '本机同时有 API Key 和官方登录，它们不在同一处。导入只会收入当前检测为生效的那一份；另一份仍留在本机。';

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
  agentId: AgentId,
  t?: TranslateFn,
): string | null {
  if (!probe) return null;
  if (probeIsAdapterProjection(probe)) return null;
  const kind = liveAuthProbeKind(probe);
  const also = alsoPresentKinds(probe);
  const alsoHasOAuth = also.some(isOAuthLiveAuthKind);
  const alsoHasApiKey = also.some(isApiKeyLiveAuthKind);
  if (kind !== 'mixed' && !alsoHasOAuth && !alsoHasApiKey) return null;

  if (agentId === 'pi' || kind === 'mixed') {
    return t ? t('connections.list.coexistPi') : 'Pi 的 auth.json 里同时有 API Key 槽和 OAuth 槽。导入会按 provider 分行，不会猜一个全局当前项。';
  }
  if (agentId === 'claude' && isApiKeyLiveAuthKind(kind) && alsoHasOAuth) {
    return t
      ? t('connections.list.coexistClaude')
      : '本机同时有 API Key 和官方登录。Claude 会优先用 Key（按 API 计费），订阅登录被压住。导入只会收入当前这份 Key。要用订阅请先去掉 settings/环境里的 Key。';
  }
  if (agentId === 'grok' && isApiKeyLiveAuthKind(kind) && alsoHasOAuth) {
    return t
      ? t('connections.list.coexistGrok')
      : '本机同时有配置里的 API Key 和 grok login 登录态。写在模型上的 Key 优先于登录态；只有全局 XAI_API_KEY 时登录态才优先。导入按当前检测结果。';
  }
  if (agentId === 'kimi') {
    return t
      ? t('connections.list.coexistKimi')
      : '本机同时有 config.toml 里的 API Key 和 /login 登录态。当前用哪一份取决于 default_model / 最后一次 /login。导入只会收入当前检测为生效的那一份。';
  }
  if (agentId === 'codex') {
    return t
      ? t('connections.list.coexistCodex')
      : '本机同时有 API Key 和 ChatGPT 登录痕迹。交互 TUI 多半仍认登录态；导入按当前检测结果。';
  }
  return t ? t('connections.list.coexistGeneric') : GENERIC_LIVE_AUTH_COEXISTENCE_NOTICE;
}

/**
 * Incomplete pool data must not be stamped as “already evaluated”.
 * A later successful refresh of the failed side should still be able to
 * surface a real first-time discovery.
 */
export function isLiveAuthDiscoveryDeferred(input: {
  poolState: ConnectionPoolDiscoveryState;
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

export function liveAuthDiscoveryKind(input: {
  poolState: ConnectionPoolDiscoveryState;
  probe?: Pick<LiveAuthProbeLike, 'kind' | 'hasCredentials' | 'isAdapterProjection' | 'alsoPresent'> | null;
  accounts: readonly { kind: string }[];
  providers: readonly unknown[];
  accountsFailed?: boolean;
  providersFailed?: boolean;
}): DiscoveredAuthKind | null {
  if (isLiveAuthDiscoveryDeferred(input)) return null;
  if (!input.probe?.hasCredentials) return null;
  if (probeIsAdapterProjection(input.probe)) return null;

  const kind = liveAuthProbeKind(input.probe);
  const hasExistingOAuth = input.accounts.some((account) => account.kind === 'oauth');
  const hasExistingApiKey =
    input.accounts.some((account) => account.kind === 'apikey') || input.providers.length > 0;

  if (isOAuthLiveAuthKind(kind) && !hasExistingOAuth) return 'account';
  if (isApiKeyLiveAuthKind(kind) && !hasExistingApiKey) return 'provider';
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
