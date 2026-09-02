/**
 * Ticket card chips, pool extras, and detail panel fields (Connections page).
 */
import { agentDisplayName, resolveAgentMeta } from '@/config/agents';
import type { LiveOccupancyDto } from '@/lib/backend/contracts/agent-catalog-types';
import { isCatalogAppendOccupancy } from '@/lib/backend/contracts/agent-catalog-types';
import { oauthListAction, type AccountAction } from '@/lib/backend/contracts/account-actions';
import {
  extractProviderCredentialFiles,
  type CredentialFileView,
} from '@/lib/credential-files';
import type { Account, AgentKey, AuthStatus, Provider } from '@/lib/types';
import type {
  BindingRoute,
  BindingView,
  TicketCredentialClass,
  TicketSurface,
  TicketView,
} from '@/lib/backend/contracts/ticket';
import {
  ticketCredentialClassLabel,
  ticketSurfaceLabel,
} from '@/lib/backend/contracts/ticket';
import {
  accountEndpointExtras,
  providerEndpointExtras,
  toCredentialRow,
} from '@/lib/credential-row';
import {
  formatRouteEndpointHttpUrl,
  routeEndpointPathForBinding,
} from '@/lib/route-endpoints';
import type { TranslateFn } from '@/lib/i18n';
import { localizeStoredUiCopy } from '@/lib/i18n/stored-copy';
import { connectionStateRouteLabel } from '@/lib/ticket-wallet-labels';

function bindingDashboardRouteLabel(route: BindingRoute, t?: TranslateFn): string {
  return connectionStateRouteLabel(route, t);
}

const IN_USE_TIP_FALLBACK = '这份登录已在当前工具使用中';
const IN_CATALOG_TIP_FALLBACK = '这份登录已经出现在模型列表里';
const SWITCH_BUSY_TIP_FALLBACK = '正在切换其他登录';
const REFRESH_BUSY_TIP_FALLBACK = '正在刷新其他登录';

export function localizeQuotaResetIn(raw: string | undefined, t?: TranslateFn): string | undefined {
  if (!raw) return undefined;
  if (!t) return raw;
  if (raw === '即将重置' || raw === 'Resets soon') return t('connections.list.quotaResetSoon');
  const zh = raw.match(/^(.+?)\s*后重置$/);
  if (zh) return t('connections.list.quotaResetIn', { when: zh[1].trim() });
  const en = raw.match(/^Resets in\s+(.+)$/i);
  if (en) return t('connections.list.quotaResetIn', { when: en[1].trim() });
  return raw;
}

export function ticketCredentialClassChipLabel(
  cls: TicketCredentialClass,
  t?: TranslateFn,
): string {
  if (!t) return ticketCredentialClassLabel(cls);
  if (cls === 'oauth') return t('kind.oauth');
  if (cls === 'api_key') return t('kind.apikey');
  return t('connections.list.unrecognized');
}

export function ticketSurfaceChipLabel(surface: TicketSurface, t?: TranslateFn): string {
  if (!t) return ticketSurfaceLabel(surface);
  if (surface === 'kimi-code-membership') return t('connections.list.surfaceMember');
  if (surface === 'anthropic-api') return t('connections.list.surfaceOfficial');
  if (surface === 'openai-api') return t('connections.list.surfaceOpenai');
  if (surface === 'xai-api') return t('connections.list.surfaceXai');
  if (surface === 'glm-coding-plan') return t('connections.list.surfaceGlm');
  if (surface === 'deepseek-api') return t('connections.list.surfaceDeepseek');
  if (
    surface === 'codex-chatgpt-subscription'
    || surface === 'claude-subscription'
    || surface === 'grok-xai-subscription'
  ) {
    return t('connections.list.surfaceSub');
  }
  return t('connections.list.unrecognized');
}

export function hasOfficialQuotaWindow(pct: number | undefined | null): boolean {
  return typeof pct === 'number' && Number.isFinite(pct);
}

/** Optional pool-row fields shown only in the ticket detail panel. */
export interface TicketDetailExtras {
  identity?: string;
  accountProvider?: string;
  endpointMode?: 'official' | 'custom';
  endpointHost?: string;
  /** Full upstream endpoint URL when known (custom endpoints). */
  endpointUrl?: string;
  authLabel?: string;
  authStatus?: AuthStatus;
  quota5hPct?: number;
  quota7dPct?: number;
  quotaResetIn?: string;
  quota7dResetIn?: string;
  canEditKey?: boolean;
  canEditConfig?: boolean;
  isCurrent?: boolean;
  oauthAction?: AccountAction;
  refreshTokenPreview?: string;
  /** `**XXXX` chip replacing 可续期 / 已配置 when a secret tail is known. */
  secretTail?: string;
  /** Pool-row display title (email when identity heal succeeded). */
  accountLabel?: string;
  subscription?: string;
  lastUsedAt?: string;
  createdAt?: string;
  tokenRemainingSec?: number;
  importedFrom?: string | null;
  /** Associated login files shown under the detail pane. */
  credentialFiles?: CredentialFileView[];
}

/** Opened OAuth details with no 5h/7d percent yet — fetch quota once. */
export function officialDetailQuotaNeedsProbe(
  extras?: Pick<TicketDetailExtras, 'oauthAction' | 'quota5hPct' | 'quota7dPct'> | null,
): boolean {
  if (!extras?.oauthAction) return false;
  return !hasOfficialQuotaWindow(extras.quota7dPct) && !hasOfficialQuotaWindow(extras.quota5hPct);
}

export interface TicketDetailField {
  label: string;
  value: string;
  mono?: boolean;
  copyable?: boolean;
}

export interface TicketDetailSections {
  /** Extra facts shown once the ticket detail panel is expanded. */
  advanced: TicketDetailField[];
  /** Protocol summary for the inspect pane; omitted from `advanced` when already listed. */
  protocol: string | null;
  /** Login validity (OAuth token remaining), shown near usage. */
  tokenRemaining: string | null;
  bindingRows: TicketBindingRowView[];
  timeline: TicketDetailField[];
  diagnostics: TicketDetailField[];
}

export interface TicketBindingDetailLine {
  agent: string;
  status: string;
}

export type TicketBindingRowView = {
  agentId: AgentKey;
  agentLabel: string;
  status: string;
  routeLabel: string | null;
  localUrl: string | null;
};

const AUTH_LABEL_HUMAN: Record<string, string> = {
  '可续期·未验证': '可续期',
  '可续期，尚未验证': '可续期',
  '已配置·未验证': '已配置',
  '已配置，尚未验证': '已配置',
  可续期: '可续期',
  已配置: '已配置',
  已验证: '已验证',
};

/** Quiet header chip: 可续期 / 已配置 / 已验证 — never 未验证 / 尚未验证. */
export function humanizeTicketAuthLabel(label: string, t?: TranslateFn): string {
  const mapped = AUTH_LABEL_HUMAN[label] ?? label.replace(/·/g, '，');
  const stripped = mapped.replace(/[·，]\s*(尚未验证|未验证)\s*$/u, '').trim() || mapped;
  return t ? localizeStoredUiCopy(stripped, t) : stripped;
}

const SECRET_TAIL_HEALTH = new Set(['可续期', '已配置', 'Renewable', 'Configured']);

/** Card chip: secret tail (`**JF6Q`) in place of 可续期 / 已配置 when known. */
export function ticketAuthChip(
  extras?: TicketDetailExtras | null,
  t?: TranslateFn,
): { label: string; mono: boolean } | null {
  if (!extras) return null;
  const health = extras.authLabel ? humanizeTicketAuthLabel(extras.authLabel, t) : '';
  const tail = extras.secretTail?.trim();
  const healthKey = extras.authLabel ? humanizeTicketAuthLabel(extras.authLabel) : '';
  if (tail && (!healthKey || SECRET_TAIL_HEALTH.has(healthKey))) {
    return { label: tail, mono: true };
  }
  if (health) return { label: health, mono: false };
  return null;
}

export type TicketSwitchChip = {
  kind: 'switch' | 'in-use';
  label: string;
};

function isPlaceholderOAuthLabel(label: string): boolean {
  const t = label.trim().toLowerCase();
  return (
    !t
    || t === '官方未提供账号信息'
    || t === '官方未提供登录信息'
    || t === 'codex-oauth'
    || t === 'codex oauth'
    || t === 'grok-oauth'
    || t === 'kimi-oauth'
    || t === 'claude-oauth'
    || t === 'pi-auth'
    || /\(oauth\)$/i.test(t)
    || / · oauth$/i.test(t)
    || / oauth$/i.test(t)
    || /-oauth$/i.test(t)
  );
}

/** Card title prefers healed account email over placeholder ticket labels. */
export function ticketCardTitle(
  ticket: Pick<TicketView, 'label'>,
  extras?: TicketDetailExtras | null,
): string {
  const identity = extras?.identity?.trim();
  if (identity && identity.includes('@')) return identity;
  const fromAccount = extras?.accountLabel?.trim();
  if (fromAccount && !isPlaceholderOAuthLabel(fromAccount)) return fromAccount;
  return ticket.label;
}

/** Native 切换 applies to the ticket's owner Agent, not a foreign usage tab. */
export function showsNativeSwitch(
  ticketAgentId: AgentKey,
  agentFilterId?: AgentKey | null,
): boolean {
  return !agentFilterId || agentFilterId === ticketAgentId;
}

export type TicketSwitchChipOpts = {
  occupancy?: LiveOccupancyDto | null;
  agentName?: string;
};

/** Card action: unused → 切换; current live grant → disabled 使用中.
 * Catalog-append occupancy uses 写入 {name} / 已在模型列表里. */
export function ticketSwitchChip(
  extras?: Pick<TicketDetailExtras, 'isCurrent'> | null,
  t?: TranslateFn,
  opts?: TicketSwitchChipOpts,
): TicketSwitchChip {
  const catalog = isCatalogAppendOccupancy(opts?.occupancy);
  if (extras?.isCurrent) {
    return {
      kind: 'in-use',
      label: catalog
        ? (t ? t('connections.list.inCatalog') : '已在模型列表里')
        : (t ? t('connections.list.inUse') : '使用中'),
    };
  }
  if (catalog) {
    const name = opts?.agentName?.trim() || 'Agent';
    return {
      kind: 'switch',
      label: t ? t('connections.list.writeCatalog', { name }) : `写入 ${name}`,
    };
  }
  return { kind: 'switch', label: t ? t('connections.list.switch') : '切换' };
}


export function ticketSwitchDisabledReason(
  input: {
    kind: TicketSwitchChip['kind'];
    switchBusy: boolean;
    canSwitch: boolean;
    occupancy?: LiveOccupancyDto | null;
  },
  t?: TranslateFn,
): string | undefined {
  if (input.kind === 'in-use') {
    if (isCatalogAppendOccupancy(input.occupancy)) {
      return t ? t('connections.list.inCatalogTip') : IN_CATALOG_TIP_FALLBACK;
    }
    return t ? t('connections.list.inUseTip') : IN_USE_TIP_FALLBACK;
  }
  if (input.switchBusy) {
    return t ? t('connections.list.switchBusyTip') : SWITCH_BUSY_TIP_FALLBACK;
  }
  if (!input.canSwitch) {
    return t ? t('connections.list.switchBusyTip') : SWITCH_BUSY_TIP_FALLBACK;
  }
  return undefined;
}

export function ticketRefreshDisabledReason(
  input: { refreshing: boolean; refreshLocked: boolean; busyLabel?: string },
  t?: TranslateFn,
): string | undefined {
  if (input.refreshing) {
    return input.busyLabel ?? (t ? t('connections.list.refreshing') : '刷新中…');
  }
  if (input.refreshLocked) {
    return t ? t('connections.list.refreshBusyTip') : REFRESH_BUSY_TIP_FALLBACK;
  }
  return undefined;
}

function endpointHostOnly(host: string): string {
  try {
    if (/^https?:\/\//i.test(host)) return new URL(host).host;
  } catch {
    /* keep raw host */
  }
  return host;
}

function formatDetailTimestamp(raw?: string | null): string | null {
  if (!raw?.trim()) return null;
  const value = raw.trim();
  const parsed = new Date(value.includes('T') ? value : value.replace(' ', 'T'));
  if (Number.isNaN(parsed.getTime())) return value;
  const y = parsed.getFullYear();
  const m = String(parsed.getMonth() + 1).padStart(2, '0');
  const d = String(parsed.getDate()).padStart(2, '0');
  const hh = String(parsed.getHours()).padStart(2, '0');
  const mm = String(parsed.getMinutes()).padStart(2, '0');
  return `${y}-${m}-${d} ${hh}:${mm}`;
}

function formatTokenRemainingLabel(sec: number | undefined, t?: TranslateFn): string | null {
  if (typeof sec !== 'number' || !Number.isFinite(sec)) return null;
  if (sec <= 0) return t ? t('connections.list.tokenExpired') : '已过期';
  const totalMin = Math.floor(sec / 60);
  if (totalMin < 60) {
    const n = Math.max(1, totalMin);
    return t ? t('connections.list.tokenRemainingMinutes', { n }) : `约 ${n} 分钟`;
  }
  const hours = Math.floor(totalMin / 60);
  if (hours < 48) {
    return t ? t('connections.list.tokenRemainingHours', { n: hours }) : `约 ${hours} 小时`;
  }
  const days = Math.floor(hours / 24);
  return t ? t('connections.list.tokenRemainingDays', { n: days }) : `约 ${days} 天`;
}

export function ticketBindingStatus(binding: BindingView, t?: TranslateFn): string {
  if (binding.route === 'bridge') {
    if (binding.bridge?.running) {
      return t ? t('connections.list.bridgeRunning') : '本机路由运行中';
    }
    if (binding.bridge && !binding.bridge.running) {
      return t ? t('connections.list.bridgeStopped') : '本机路由已停止';
    }
  }
  if (binding.active) return t ? t('connections.list.currentlyUsed') : '当前使用';
  return t ? t('connections.list.unused') : '未使用';
}

export function findTicketPoolSource(
  ticket: Pick<TicketView, 'sourceKind' | 'sourceId' | 'agentId'>,
  accounts: readonly Account[],
  providers: readonly Provider[],
): { account?: Account; provider?: Provider } {
  if (ticket.sourceKind === 'provider') {
    const provider =
      providers.find((item) => item.id === ticket.sourceId && item.agentId === ticket.agentId)
      ?? providers.find((item) => item.id === ticket.sourceId);
    return { provider };
  }
  const account =
    accounts.find((item) => item.id === ticket.sourceId && item.agentId === ticket.agentId)
    ?? accounts.find((item) => item.id === ticket.sourceId);
  return { account };
}

export function extrasFromPoolSource(
  ticket: TicketView,
  source: { account?: Account; provider?: Provider },
  t?: TranslateFn,
  tabCurrentTicketId?: string | null,
): TicketDetailExtras {
  const poolCurrent = source.account?.isCurrent === true || source.provider?.isCurrent === true;
  const liveCatalog = source.account?.source === 'live';
  const catalog = isCatalogAppendOccupancy(resolveAgentMeta(ticket.agentId).occupancy);
  const extras: TicketDetailExtras = {
    canEditKey: ticket.sourceKind === 'account' && source.account?.kind === 'apikey',
    canEditConfig: ticket.sourceKind === 'provider' && Boolean(source.provider),
    // Catalog-append tools keep many live rows. "in catalog" is not the
    // exclusive current pointer: a previously written row stays listed.
    isCurrent: catalog
      ? poolCurrent
        || liveCatalog
        || (tabCurrentTicketId != null && ticket.id === tabCurrentTicketId)
      : tabCurrentTicketId === undefined
        ? poolCurrent
        : ticket.id === tabCurrentTicketId,
  };

  if (source.account) {
    const row = toCredentialRow({ source: 'account', account: source.account }, t);
    extras.identity =
      ticket.credentialClass === 'oauth'
        ? source.account.email
          ?? source.account.identityLabel
          ?? source.account.subjectId
          ?? (t ? t('connections.list.noAccountInfo') : '官方未提供登录信息')
        : source.account.email ?? source.account.identityLabel ?? source.account.label;
    if (
      source.account.provider
      && source.account.provider !== source.account.agentId
      && !/^https?:\/\//i.test(source.account.provider)
      && typeof ticket.label === 'string'
      && !ticket.label.includes(source.account.provider)
    ) {
      extras.accountProvider = source.account.provider;
    }
    extras.authLabel = row.auth.label;
    extras.authStatus = row.auth.status;
    extras.quota5hPct = source.account.quota5hPct;
    extras.quota7dPct = source.account.quota7dPct;
    extras.quotaResetIn = localizeQuotaResetIn(source.account.quotaResetIn, t);
    extras.quota7dResetIn = localizeQuotaResetIn(source.account.quota7dResetIn, t);
    const endpoint = accountEndpointExtras(source.account);
    extras.endpointMode = endpoint.endpointMode;
    extras.endpointHost = endpoint.endpointHost;
    extras.endpointUrl = endpoint.endpoint;
    extras.oauthAction = oauthListAction(source.account);
    if (ticket.credentialClass === 'oauth' && source.account.refreshTokenPreview) {
      extras.refreshTokenPreview = source.account.refreshTokenPreview;
    }
    if (source.account.secretTail) {
      extras.secretTail = source.account.secretTail;
    }
    extras.accountLabel = source.account.label;
    if (source.account.subscription?.trim()) {
      extras.subscription = source.account.subscription.trim();
    }
    if (source.account.lastUsedAt?.trim()) {
      extras.lastUsedAt = source.account.lastUsedAt.trim();
    }
    if (source.account.createdAt?.trim()) {
      extras.createdAt = source.account.createdAt.trim();
    }
    if (typeof source.account.tokenRemainingSec === 'number') {
      extras.tokenRemainingSec = source.account.tokenRemainingSec;
    }
    if (source.account.credentialFiles?.length) {
      extras.credentialFiles = source.account.credentialFiles;
    }
  }

  if (source.provider) {
    const row = toCredentialRow({ source: 'provider', provider: source.provider }, t);
    const endpoint = providerEndpointExtras(source.provider);
    extras.endpointMode = endpoint.endpointMode;
    extras.endpointHost = endpoint.endpointHost;
    extras.endpointUrl = endpoint.endpoint;
    extras.authLabel = row.auth.label;
    extras.authStatus = row.auth.status;
    if (source.provider.secretTail) {
      extras.secretTail = source.provider.secretTail;
    }
    if (source.provider.updatedAt?.trim()) {
      extras.createdAt = source.provider.updatedAt.trim();
    }
    if (!extras.credentialFiles?.length) {
      extras.credentialFiles = extractProviderCredentialFiles(source.provider);
    }
  }

  if (ticket.importedFrom?.trim()) {
    extras.importedFrom = ticket.importedFrom.trim();
  }

  return extras;
}

/**
 * Advanced-only facts for the ticket detail expand.
 * Header already shows type / surface / health chip.
 */
function protocolLabel(speaks: readonly string[]): string {
  const names: Record<string, string> = {
    'openai-chat': 'Chat',
    'anthropic-messages': 'Claude',
    'openai-responses': 'Codex',
    'grok-responses': 'Grok',
    'xai-device-code': 'Grok',
  };
  const seen = new Set<string>();
  const out: string[] = [];
  for (const item of speaks) {
    const key = item.trim().toLowerCase();
    if (!key) continue;
    const label = names[key];
    if (!label || seen.has(label)) continue;
    seen.add(label);
    out.push(label);
  }
  return out.join(' · ');
}

function localRouteClientLabel(agentId: BindingView['agentId']): string {
  if (agentId === 'claude') return 'Claude';
  if (agentId === 'codex') return 'Codex';
  if (agentId === 'grok') return 'Grok';
  if (agentId === 'kimi') return 'Kimi';
  return agentDisplayName(agentId);
}

function localRouteSurface(
  bindings?: readonly BindingView[] | null,
): string | null {
  if (!bindings?.length) return null;
  const labels: string[] = [];
  const seen = new Set<string>();
  for (const binding of bindings) {
    if (binding.route !== 'bridge') continue;
    const label = localRouteClientLabel(binding.agentId);
    if (seen.has(label)) continue;
    seen.add(label);
    labels.push(label);
  }
  return labels.length > 0 ? labels.join(' · ') : null;
}

export function buildTicketDetailFields(
  ticket: TicketView,
  extras?: TicketDetailExtras | null,
  t?: TranslateFn,
  bindings?: readonly BindingView[] | null,
): TicketDetailSections {
  const advanced: TicketDetailField[] = [];

  if (isCatalogAppendOccupancy(resolveAgentMeta(ticket.agentId).occupancy)) {
    advanced.push({
      label: t ? t('connections.list.catalogStatus') : '模型列表',
      value: extras?.isCurrent
        ? (t ? t('connections.list.inCatalog') : '已在模型列表里')
        : (t ? t('connections.list.notInCatalog') : '未写入模型列表'),
    });
  }

  const providerName = extras?.accountProvider?.trim();
  if (
    ticket.credentialClass === 'api_key'
    && providerName
    && providerName !== ticket.agentId
    && !/^https?:\/\//i.test(providerName)
  ) {
    advanced.push({
      label: t ? t('connections.list.provider') : '供应商',
      value: providerName,
    });
  }

  const customEndpoint = extras != null && extras.endpointMode === 'custom';
  if (customEndpoint) {
    advanced.push({
      label: t ? t('connections.list.endpoint') : '端点',
      value: t ? t('connections.list.customEndpoint') : '自定义端点',
    });
    const address = extras.endpointUrl?.trim()
      || (extras.endpointHost ? endpointHostOnly(extras.endpointHost) : '');
    if (address) {
      advanced.push({
        label: t ? t('connections.list.endpointAddress') : '地址',
        value: address,
        mono: true,
        copyable: true,
      });
    }
  }

  if (extras?.subscription?.trim()) {
    advanced.push({
      label: t ? t('connections.list.subscription') : '套餐',
      value: extras.subscription.trim(),
    });
  }

  const speaks = Array.isArray(ticket.speaks) ? ticket.speaks : [];
  const customApiKey = ticket.credentialClass === 'api_key' && customEndpoint;
  const interfaceLabel = protocolLabel(speaks);
  if (customApiKey && interfaceLabel) {
    advanced.push({
      label: t ? t('connections.list.protocol') : '接口',
      value: interfaceLabel,
    });
  }

  const agentSurface = localRouteSurface(bindings);
  if (agentSurface) {
    advanced.push({
      label: t ? t('kind.route.localRoute') : '本机路由',
      value: agentSurface,
      mono: true,
    });
  }

  const timeline: TicketDetailField[] = [];
  const lastUsed = formatDetailTimestamp(extras?.lastUsedAt);
  if (lastUsed) {
    timeline.push({
      label: t ? t('connections.list.lastUsedAt') : '最近使用',
      value: lastUsed,
    });
  }
  const created = formatDetailTimestamp(extras?.createdAt);
  if (created) {
    timeline.push({
      label: t ? t('connections.list.createdAt') : '添加时间',
      value: created,
    });
  }

  const diagnostics: TicketDetailField[] = [];
  if (extras?.importedFrom?.trim()) {
    diagnostics.push({
      label: t ? t('connections.list.importedFrom') : '导入来源',
      value: extras.importedFrom.trim(),
    });
  }
  if (ticket.id.trim()) {
    diagnostics.push({
      label: t ? t('connections.list.ticketId') : '记录 ID',
      value: ticket.id,
      mono: true,
      copyable: true,
    });
  }

  return {
    advanced,
    protocol: interfaceLabel || null,
    tokenRemaining: formatTokenRemainingLabel(extras?.tokenRemainingSec, t),
    bindingRows: buildTicketBindingRows(bindings, t),
    timeline,
    diagnostics,
  };
}

export function buildTicketBindingRows(
  bindings: readonly BindingView[] | null | undefined,
  t?: TranslateFn,
): TicketBindingRowView[] {
  if (!bindings?.length) return [];
  return bindings.map((binding) => ({
    agentId: binding.agentId,
    agentLabel: agentDisplayName(binding.agentId),
    status: ticketBindingStatus(binding, t),
    routeLabel: binding.route === 'native'
      ? null
      : bindingDashboardRouteLabel(binding.route, t),
    localUrl: binding.route === 'bridge'
      ? formatRouteEndpointHttpUrl({
          path: routeEndpointPathForBinding({ agentId: binding.agentId }),
          port: binding.bridge?.port ?? null,
        })
      : null,
  }));
}

export function formatTicketBindingDetailLines(
  bindings: readonly BindingView[],
  t?: TranslateFn,
): TicketBindingDetailLine[] {
  return bindings.map((binding) => ({
    agent: binding.route === 'bridge'
      ? formatRouteEndpointHttpUrl({
          path: routeEndpointPathForBinding({ agentId: binding.agentId }),
          port: binding.bridge?.port ?? null,
        })
      : agentDisplayName(binding.agentId),
    status: ticketBindingStatus(binding, t),
  }));
}

export function ticketDetailEditLabel(
  extras?: TicketDetailExtras | null,
  t?: TranslateFn,
): string | null {
  if (extras?.canEditConfig) return t ? t('connections.list.editConfig') : '编辑配置';
  if (extras?.canEditKey) return t ? t('connections.list.editKey') : '编辑密钥';
  return null;
}
