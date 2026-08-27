/**
 * Global ticket-wallet list helpers (Connections page).
 * Filter / binding usage lines — pure functions for vitest.
 */
import { agentSupportsOAuthLogin } from '@/lib/backend/contracts/oauth-constants';
import { agentDisplayName } from '@/config/agents';
import {
  formatRouteEndpointHttpUrl,
  routeEndpointIdForBinding,
  routeEndpointPathForBinding,
  type RouteEndpointId,
} from '@/lib/route-endpoints';
import { oauthListAction, type AccountAction } from '@/lib/backend/contracts/account-actions';
import type { Account, AgentId, AuthStatus, Provider } from '@/lib/types';
import type {
  BindingRoute,
  BindingView,
  TicketCredentialClass,
  TicketSurface,
  TicketView,
  TicketWallet,
} from '@/lib/backend/contracts/ticket';
import {
  bindingRouteDashboardLabel,
  surfaceGroupMemberCount,
  ticketCredentialClassLabel,
  ticketSurfaceLabel,
} from '@/lib/backend/contracts/ticket';
import {
  providerEndpointExtras,
  toCredentialRow,
} from '@/lib/credential-row';
import { bridgesHrefForProfile } from '@/lib/bridges-path';
import { bindRouteMatchesPurpose } from '@/lib/connect-flow/types';
import type { TranslateFn } from '@/lib/i18n';
import {
  activeBindingForAgent,
  filterTicketsByAgentUsage,
} from '@/lib/ticket-wallet';

export { activeBindingForAgent, filterTicketsByAgentUsage };

/**
 * Hide omitted agents' own tickets unless an installed agent still has an
 * active binding to them. Bindings to omitted agents always drop.
 */
export function filterWalletByExcludedAgents(
  wallet: TicketWallet | null,
  excludedIds: Iterable<string>,
): TicketWallet | null {
  if (!wallet) return null;
  const excluded = excludedIds instanceof Set ? excludedIds : new Set(excludedIds);
  if (excluded.size === 0) return wallet;
  const tickets = wallet.tickets.filter((ticket) => {
    if (!excluded.has(ticket.agentId)) return true;
    return wallet.bindings.some(
      (binding) =>
        binding.ticketId === ticket.id &&
        binding.active &&
        !excluded.has(binding.agentId),
    );
  });
  const ticketIds = new Set(tickets.map((ticket) => ticket.id));
  const bindings = wallet.bindings.filter(
    (binding) => ticketIds.has(binding.ticketId) && !excluded.has(binding.agentId),
  );
  const surfaceGroups = (wallet.surfaceGroups ?? [])
    .map((group) => ({
      ...group,
      members: group.members.filter((member) => ticketIds.has(member.ticketId)),
    }))
    .filter((group) => group.members.length > 0);
  return { ...wallet, tickets, bindings, surfaceGroups };
}

export type TicketWalletFilter = 'all' | TicketCredentialClass;

export const TICKET_WALLET_FILTERS: Array<{ value: TicketWalletFilter; label: string }> = [
  { value: 'all', label: '全部' },
  { value: 'oauth', label: '官方登录' },
  { value: 'api_key', label: 'API Key' },
  { value: 'unknown', label: '未识别' },
];

export type TicketAddKind = 'import-login' | 'oauth' | 'api-key';

export const TICKET_ADD_ACTIONS: Array<{ kind: TicketAddKind; label: string }> = [
  { kind: 'import-login', label: '导入当前登录' },
  { kind: 'oauth', label: '官方登录' },
  { kind: 'api-key', label: '添加 API Key' },
];

export function ticketAddActionsForAgent(agentId: AgentId): Array<{ kind: TicketAddKind; label: string }> {
  if (agentSupportsOAuthLogin(agentId)) return TICKET_ADD_ACTIONS;
  return TICKET_ADD_ACTIONS.filter((item) => item.kind !== 'oauth');
}

export function ticketWalletFilterLabel(
  filter: TicketWalletFilter,
  t?: TranslateFn,
): string {
  if (!t) {
    return TICKET_WALLET_FILTERS.find((item) => item.value === filter)?.label ?? '全部';
  }
  if (filter === 'all') return t('kind.all');
  if (filter === 'oauth') return t('kind.oauth');
  if (filter === 'api_key') return t('kind.apikey');
  return t('connections.list.unrecognized');
}

export function ticketAddActionLabel(kind: TicketAddKind, t?: TranslateFn): string {
  if (!t) {
    return TICKET_ADD_ACTIONS.find((item) => item.kind === kind)?.label ?? '导入当前登录';
  }
  if (kind === 'import-login') return t('connections.list.importLogin');
  if (kind === 'oauth') return t('connections.list.addOauth');
  return t('connections.list.addApiKey');
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

/** Same connection-state words on Connections cards and Dashboard agent cards. */
export function connectionStateRouteLabel(route: BindingRoute, t?: TranslateFn): string {
  if (!t) return bindingRouteDashboardLabel(route);
  if (route === 'reshape') return t('connections.list.routeReshape');
  if (route === 'bridge') return t('kind.route.localRoute');
  return t('kind.route.direct');
}

function bindingUsageRouteLabel(route: BindingRoute, t?: TranslateFn): string {
  return connectionStateRouteLabel(route, t);
}

function bindingDashboardRouteLabel(route: BindingRoute, t?: TranslateFn): string {
  return connectionStateRouteLabel(route, t);
}

export interface TicketAddMenuAgent {
  id: AgentId;
  name: string;
  actions: Array<{ kind: TicketAddKind; label: string }>;
}

export function buildTicketAddMenu(
  agentIds?: readonly AgentId[] | null,
): TicketAddMenuAgent[] {
  if (!agentIds || agentIds.length === 0) return [];
  return agentIds.map((id) => ({
    id,
    name: agentDisplayName(id),
    actions: ticketAddActionsForAgent(id),
  }));
}

/** When an Agent tab is selected, skip the agent picker and use that Agent's actions. */
export function focusedTicketAddAgent(
  agents: readonly TicketAddMenuAgent[],
  focusedAgentId?: AgentId | null,
): TicketAddMenuAgent | null {
  if (!focusedAgentId) return null;
  return agents.find((item) => item.id === focusedAgentId) ?? null;
}

export function dispatchTicketAddAction(
  kind: TicketAddKind,
  agentId: AgentId,
  handlers: {
    onImportLogin?: (id: AgentId) => void;
    onOauth?: (id: AgentId) => void;
    onAddKey?: (id: AgentId) => void;
  },
): void {
  if (kind === 'import-login') {
    handlers.onImportLogin?.(agentId);
    return;
  }
  if (kind === 'oauth') {
    handlers.onOauth?.(agentId);
    return;
  }
  handlers.onAddKey?.(agentId);
}

/** After the originating click, so menu unmount cannot dismiss the new dialog. */
export function scheduleAfterMenuClose(action: () => void, delayMs = 0): void {
  const schedule = globalThis.setTimeout;
  if (typeof schedule === 'function') {
    schedule(action, delayMs);
    return;
  }
  action();
}

/** Swallow a leftover Dialog `onOpenChange(false)` from the opening click. */
export function shouldIgnoreMenuDialogDismiss(armed: boolean, nextOpen: boolean): boolean {
  return armed && !nextOpen;
}

/** Connections `openTicketAdd` and AgentCard uninstall: clear the arm after the click settles. */
export const MENU_DIALOG_DISMISS_CLEAR_MS = 100;

type MenuDialogSchedule = (fn: () => void, delayMs?: number) => void;

/** Arm ignore-dismiss, open the dialog, then clear the arm after `delayMs`. */
export function armMenuDialogOpen(
  arm: { current: boolean },
  open: () => void,
  delayMs = MENU_DIALOG_DISMISS_CLEAR_MS,
  schedule: MenuDialogSchedule = scheduleAfterMenuClose,
): void {
  arm.current = true;
  open();
  schedule(() => {
    arm.current = false;
  }, delayMs);
}

/**
 * Menu item that opens a Dialog: preventDefault keeps the menu mounted through
 * the click; arm + delayed clear swallows the leftover dismiss.
 */
export function handleMenuDialogSelect(
  event: { preventDefault: () => void },
  arm: { current: boolean },
  open: () => void,
  delayMs = MENU_DIALOG_DISMISS_CLEAR_MS,
  schedule: MenuDialogSchedule = scheduleAfterMenuClose,
): void {
  event.preventDefault();
  armMenuDialogOpen(arm, open, delayMs, schedule);
}

/**
 * Menu item select for 导入当前登录 / 添加 API Key.
 * preventDefault keeps the menu mounted through the click so the Dialog is
 * not dismissed and the pointer cannot hit the segmented filter underneath.
 * Close is delayed until after the click settles — timeout 0 unmounts the
 * submenu in time for the same click to hit AgentTabStrip (silence).
 */
export function handleTicketAddMenuSelect(
  event: { preventDefault: () => void; stopPropagation?: () => void },
  kind: TicketAddKind,
  agentId: AgentId,
  handlers: {
    onImportLogin?: (id: AgentId) => void;
    onOauth?: (id: AgentId) => void;
    onAddKey?: (id: AgentId) => void;
    onMenuClose?: () => void;
  },
  schedule: (fn: () => void, delayMs?: number) => void = scheduleAfterMenuClose,
  closeDelayMs = MENU_DIALOG_DISMISS_CLEAR_MS,
): void {
  event.preventDefault();
  event.stopPropagation?.();
  dispatchTicketAddAction(kind, agentId, handlers);
  if (handlers.onMenuClose) schedule(handlers.onMenuClose, closeDelayMs);
}

export function ticketAddDialogState(
  kind: TicketAddKind,
  agentId: AgentId,
): {
  addAgentId: AgentId;
  loginImportOpen: boolean;
  oauthDialogOpen: boolean;
  apiKeyDialogOpen: boolean;
  clearEditProvider: boolean;
} {
  return {
    addAgentId: agentId,
    loginImportOpen: kind === 'import-login',
    oauthDialogOpen: kind === 'oauth',
    apiKeyDialogOpen: kind === 'api-key',
    clearEditProvider: kind === 'api-key',
  };
}

/** 「未识别」按 surface；可兼 credentialClass === 'unknown'。 */
export function isUnrecognizedTicket(ticket: Pick<TicketView, 'surface' | 'credentialClass'>): boolean {
  return ticket.surface === 'unknown' || ticket.credentialClass === 'unknown';
}

export type TicketUsagePart =
  | { kind: 'text'; text: string }
  | { kind: 'bridge'; label: string; href: string }
  | { kind: 'endpoint'; path: string; port: number | null; endpointId: RouteEndpointId };

export interface TicketWalletRow {
  ticket: TicketView;
  bindings: BindingView[];
  /** Active bindings for highlightAgent (deep-link ?agent=). */
  highlighted: boolean;
  usageText: string;
  usageParts: TicketUsagePart[];
}

export function bindingsForTicket(
  wallet: TicketWallet,
  ticketId: string,
): BindingView[] {
  return wallet.bindings.filter((b) => b.ticketId === ticketId);
}

export function formatBindingUsageParts(
  binding: BindingView,
  t?: TranslateFn,
  memberCount = 1,
): TicketUsagePart[] {
  const route = bindingUsageRouteLabel(binding.route, t);
  const name = agentDisplayName(binding.agentId);
  const poolSuffix = binding.route === 'bridge' && memberCount > 1
    ? (t
      ? t('connections.list.poolSuffix', { n: memberCount })
      : ` · ${memberCount} 份同类登录可轮换`)
    : '';
  if (binding.route === 'bridge') {
    const path = routeEndpointPathForBinding({ agentId: binding.agentId });
    const endpointId = routeEndpointIdForBinding({ agentId: binding.agentId });
    const port = binding.bridge?.port ?? null;
    const suffix = binding.bridge?.running
      ? `${poolSuffix}${t ? t('connections.list.runningSuffix') : ' · 运行中'}`
      : binding.bridge && !binding.bridge.running
        ? `${poolSuffix}${t ? t('connections.list.stoppedSuffix') : ' · 已停止'}`
        : poolSuffix;
    return [
      { kind: 'endpoint', path, port, endpointId },
      { kind: 'text', text: '（' },
      { kind: 'bridge', label: route, href: bridgesHrefForProfile(binding.profileId) },
      { kind: 'text', text: t ? t('connections.list.usageCloseWithSuffix', { suffix }) : `${suffix}）` },
    ];
  }
  return [{
    kind: 'text',
    text: t ? t('connections.list.usageWithRoute', { name, route }) : `${name}（${route}）`,
  }];
}

export function formatBindingUsagePart(
  binding: BindingView,
  t?: TranslateFn,
  memberCount = 1,
): string {
  return formatBindingUsageParts(binding, t, memberCount)
    .map((part) => usagePartPlainText(part))
    .join('');
}

function usagePartPlainText(part: TicketUsagePart): string {
  if (part.kind === 'bridge') return part.label;
  if (part.kind === 'endpoint') {
    return formatRouteEndpointHttpUrl({ path: part.path, port: part.port });
  }
  return part.text;
}

export function formatTicketUsageParts(
  bindings: readonly BindingView[],
  ownerAgentId?: AgentId,
  t?: TranslateFn,
  memberCount = 1,
): TicketUsagePart[] {
  const active = bindings.filter((b) => b.active);
  if (active.length === 0) {
    return [{
      kind: 'text',
      text: ownerAgentId
        ? (t
          ? t('connections.list.unusedWithOwner', { name: agentDisplayName(ownerAgentId) })
          : `${agentDisplayName(ownerAgentId)} · 未使用`)
        : (t ? t('connections.list.unused') : '未使用'),
    }];
  }
  const selfOnly =
    Boolean(ownerAgentId)
    && active.length === 1
    && active[0]!.agentId === ownerAgentId;
  if (selfOnly) {
    return formatBindingUsageParts(active[0]!, t, memberCount);
  }
  const parts: TicketUsagePart[] = [{
    kind: 'text',
    text: t ? t('connections.list.usedFor') : '正用于：',
  }];
  active.forEach((binding, index) => {
    if (index > 0) parts.push({ kind: 'text', text: ' · ' });
    parts.push(...formatBindingUsageParts(binding, t, memberCount));
  });
  return parts;
}

export function formatTicketUsageText(
  bindings: readonly BindingView[],
  ownerAgentId?: AgentId,
  t?: TranslateFn,
  memberCount = 1,
): string {
  return formatTicketUsageParts(bindings, ownerAgentId, t, memberCount)
    .map((part) => usagePartPlainText(part))
    .join('');
}

export function countTicketsByFilter(
  tickets: readonly TicketView[],
): Record<TicketWalletFilter, number> {
  const counts: Record<TicketWalletFilter, number> = {
    all: tickets.length,
    oauth: 0,
    api_key: 0,
    unknown: 0,
  };
  for (const ticket of tickets) {
    if (isUnrecognizedTicket(ticket)) {
      counts.unknown += 1;
    }
    if (ticket.credentialClass === 'oauth') counts.oauth += 1;
    else if (ticket.credentialClass === 'api_key') counts.api_key += 1;
  }
  return counts;
}

export function filterTickets(
  tickets: readonly TicketView[],
  filter: TicketWalletFilter,
): TicketView[] {
  if (filter === 'all') return [...tickets];
  if (filter === 'unknown') {
    return tickets.filter((t) => isUnrecognizedTicket(t));
  }
  return tickets.filter((t) => t.credentialClass === filter);
}

export function buildTicketWalletRows(
  wallet: TicketWallet,
  options: {
    filter?: TicketWalletFilter;
    /** Deep-link highlight for that Agent's active binding. */
    highlightAgentId?: AgentId | null;
    /** Agent tab filter; omit for the full wallet. */
    agentFilterId?: AgentId | null;
    t?: TranslateFn;
  } = {},
): TicketWalletRow[] {
  const filter = options.filter ?? 'all';
  const highlightAgentId = options.highlightAgentId ?? null;
  const agentFilterId = options.agentFilterId ?? null;
  const t = options.t;

  let tickets = filterTickets(wallet.tickets, filter);
  if (agentFilterId) {
    tickets = filterTicketsByAgentUsage(wallet, tickets, agentFilterId);
  }

  return tickets.map((ticket) => {
    const bindings = bindingsForTicket(wallet, ticket.id);
    const memberCount = surfaceGroupMemberCount(wallet.surfaceGroups, ticket.id);
    const highlighted = Boolean(
      highlightAgentId
      && bindings.some((b) => b.active && b.agentId === highlightAgentId),
    );
    return {
      ticket,
      bindings,
      highlighted,
      usageText: formatTicketUsageText(bindings, ticket.agentId, t, memberCount),
      usageParts: formatTicketUsageParts(bindings, ticket.agentId, t, memberCount),
    };
  });
}

export function dashboardBindingMetaText(
  ticketLabel: string,
  route: BindingRoute,
  t?: TranslateFn,
): string {
  return `${ticketLabel} · ${bindingDashboardRouteLabel(route, t)}`;
}

/**
 * Show a quota bar only when upstream returned a percent. `0` is a real value.
 * Codex 5h is not inferred from 7d: missing quota5hPct hides that bar.
 */
export function hasOfficialQuotaWindow(pct: number | undefined | null): boolean {
  return typeof pct === 'number' && Number.isFinite(pct);
}

/** Optional pool-row fields shown only in the ticket detail panel. */
export interface TicketDetailExtras {
  identity?: string;
  accountProvider?: string;
  endpointMode?: 'official' | 'custom';
  endpointHost?: string;
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
}

export interface TicketDetailField {
  label: string;
  value: string;
  mono?: boolean;
}

export interface TicketDetailSections {
  /** Extra facts shown once the ticket detail panel is expanded. */
  advanced: TicketDetailField[];
  /** Protocol summary for the inspect pane; omitted from `advanced` when already listed. */
  protocol: string | null;
  bindingRows: TicketBindingRowView[];
  diagnostics: TicketDetailField[];
}

export interface TicketBindingDetailLine {
  agent: string;
  status: string;
}

export type TicketBindingRowView = {
  agentId: AgentId;
  agentLabel: string;
  status: string;
  routeLabel: string;
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
export function humanizeTicketAuthLabel(label: string): string {
  const mapped = AUTH_LABEL_HUMAN[label] ?? label.replace(/·/g, '，');
  return mapped.replace(/[·，]\s*(尚未验证|未验证)\s*$/u, '').trim() || mapped;
}

const SECRET_TAIL_HEALTH = new Set(['可续期', '已配置', 'Renewable', 'Configured']);

/** Card chip: secret tail (`**JF6Q`) in place of 可续期 / 已配置 when known. */
export function ticketAuthChip(
  extras?: TicketDetailExtras | null,
): { label: string; mono: boolean } | null {
  if (!extras) return null;
  const health = extras.authLabel ? humanizeTicketAuthLabel(extras.authLabel) : '';
  const tail = extras.secretTail?.trim();
  if (tail && (!health || SECRET_TAIL_HEALTH.has(health))) {
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
  ticketAgentId: AgentId,
  agentFilterId?: AgentId | null,
): boolean {
  return !agentFilterId || agentFilterId === ticketAgentId;
}

/** Card action: unused → 切换; current live grant → disabled 使用中. */
export function ticketSwitchChip(
  extras?: Pick<TicketDetailExtras, 'isCurrent'> | null,
  t?: TranslateFn,
): TicketSwitchChip {
  if (extras?.isCurrent) {
    return { kind: 'in-use', label: t ? t('connections.list.inUse') : '使用中' };
  }
  return { kind: 'switch', label: t ? t('connections.list.switch') : '切换' };
}

/** Settled plan snapshot for row bind buttons. */
export type TicketRoutePlanHint = {
  status: 'pending' | 'blocked_oauth' | 'ready' | 'error';
  route?: 'native_endpoint' | 'local_bridge' | 'config_sync' | 'unsupported';
  canApply?: boolean;
  reason?: string;
};

export type TicketBindPurpose = 'share' | 'route';

export type TicketBindAction =
  | { disabled: false }
  | { disabled: true; reason: string };

/** @deprecated use TicketBindAction */
export type TicketRouteAction = TicketBindAction;

const ROUTE_DISABLED_FALLBACK = '这份登录目前不能走本机转发';
const ROUTE_NO_TARGET_FALLBACK = '没有可转发的目标工具';
const SHARE_DISABLED_FALLBACK = '这份登录目前不能直接用到其它工具';
const SHARE_NO_TARGET_FALLBACK = '没有可接到的目标工具';
const IN_USE_TIP_FALLBACK = '这份登录已在当前工具使用中';
const SWITCH_BUSY_TIP_FALLBACK = '正在切换其他登录';
const REFRESH_BUSY_TIP_FALLBACK = '正在刷新其他登录';

/** Share = 直连 / 写进对方登录. Route = 本机转发 only. */
export function ticketRouteMatchesPurpose(
  route: TicketRoutePlanHint['route'],
  purpose: TicketBindPurpose,
): boolean {
  return bindRouteMatchesPurpose(route, purpose);
}

/** Disable a bind button only after every target settled with no applyable matching route. */
export function resolveTicketBindAction(
  hints: readonly TicketRoutePlanHint[],
  purpose: TicketBindPurpose,
  t?: TranslateFn,
): TicketBindAction {
  const noTarget = purpose === 'route'
    ? (t ? t('connections.list.routeNoTarget') : ROUTE_NO_TARGET_FALLBACK)
    : (t ? t('connections.list.shareNoTarget') : SHARE_NO_TARGET_FALLBACK);
  const generic = purpose === 'route'
    ? (t ? t('connections.list.routeDisabled') : ROUTE_DISABLED_FALLBACK)
    : (t ? t('connections.list.shareDisabled') : SHARE_DISABLED_FALLBACK);

  if (hints.length === 0) return { disabled: true, reason: noTarget };

  let pending = false;
  let sawConclusive = false;
  let matchingBlockedReason: string | undefined;
  let oauthReason: string | undefined;

  for (const hint of hints) {
    if (hint.status === 'pending') {
      pending = true;
      continue;
    }
    if (hint.status === 'ready' && ticketRouteMatchesPurpose(hint.route, purpose) && hint.canApply) {
      return { disabled: false };
    }
    if (hint.status === 'ready' && ticketRouteMatchesPurpose(hint.route, purpose) && !hint.canApply) {
      sawConclusive = true;
      matchingBlockedReason ??= hint.reason;
      continue;
    }
    if (hint.status === 'blocked_oauth') {
      sawConclusive = true;
      oauthReason ??= hint.reason;
      continue;
    }
    if (hint.status === 'ready') {
      sawConclusive = true;
    }
  }

  if (pending || !sawConclusive) return { disabled: false };
  if (matchingBlockedReason) return { disabled: true, reason: matchingBlockedReason };
  if (oauthReason) return { disabled: true, reason: oauthReason };
  return { disabled: true, reason: generic };
}

export function resolveTicketRouteAction(
  hints: readonly TicketRoutePlanHint[],
  t?: TranslateFn,
): TicketBindAction {
  return resolveTicketBindAction(hints, 'route', t);
}

export function resolveTicketShareAction(
  hints: readonly TicketRoutePlanHint[],
  t?: TranslateFn,
): TicketBindAction {
  return resolveTicketBindAction(hints, 'share', t);
}

export function ticketSwitchDisabledReason(
  input: { kind: TicketSwitchChip['kind']; switchBusy: boolean; canSwitch: boolean },
  t?: TranslateFn,
): string | undefined {
  if (input.kind === 'in-use') {
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
  const extras: TicketDetailExtras = {
    canEditKey: ticket.sourceKind === 'account' && source.account?.kind === 'apikey',
    canEditConfig: ticket.sourceKind === 'provider' && Boolean(source.provider),
    isCurrent: tabCurrentTicketId === undefined
      ? source.account?.isCurrent === true || source.provider?.isCurrent === true
      : ticket.id === tabCurrentTicketId,
  };

  if (source.account) {
    const row = toCredentialRow({ source: 'account', account: source.account });
    extras.identity =
      ticket.credentialClass === 'oauth'
        ? source.account.email
          ?? source.account.identityLabel
          ?? source.account.subjectId
          ?? (t ? t('connections.list.noAccountInfo') : '官方未提供账号信息')
        : source.account.email ?? source.account.identityLabel ?? source.account.label;
    if (
      source.account.provider
      && typeof ticket.label === 'string'
      && !ticket.label.includes(source.account.provider)
    ) {
      extras.accountProvider = source.account.provider;
    }
    extras.authLabel = row.auth.label;
    extras.authStatus = row.auth.status;
    extras.quota5hPct = source.account.quota5hPct;
    extras.quota7dPct = source.account.quota7dPct;
    extras.quotaResetIn = source.account.quotaResetIn;
    extras.quota7dResetIn = source.account.quota7dResetIn;
    extras.endpointMode = source.account.kind === 'apikey' ? 'official' : undefined;
    extras.oauthAction = oauthListAction(source.account);
    if (ticket.credentialClass === 'oauth' && source.account.refreshTokenPreview) {
      extras.refreshTokenPreview = source.account.refreshTokenPreview;
    }
    if (source.account.secretTail) {
      extras.secretTail = source.account.secretTail;
    }
    extras.accountLabel = source.account.label;
  }

  if (source.provider) {
    const row = toCredentialRow({ source: 'provider', provider: source.provider });
    const endpoint = providerEndpointExtras(source.provider);
    extras.endpointMode = endpoint.endpointMode;
    extras.endpointHost = endpoint.endpointHost;
    extras.authLabel = row.auth.label;
    extras.authStatus = row.auth.status;
    if (source.provider.secretTail) {
      extras.secretTail = source.provider.secretTail;
    }
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

  const customEndpoint = extras != null && extras.endpointMode === 'custom';
  if (customEndpoint) {
    advanced.push({
      label: t ? t('connections.list.endpoint') : '端点',
      value: t ? t('connections.list.custom') : '自定义',
    });
    if (extras.endpointHost) {
      advanced.push({
        label: t ? t('connections.list.endpointHost') : '主机',
        value: endpointHostOnly(extras.endpointHost),
        mono: true,
      });
    }
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

  const diagnostics: TicketDetailField[] = [];
  if (ticket.id.trim()) {
    diagnostics.push({
      label: t ? t('connections.list.ticketId') : '记录 ID',
      value: ticket.id,
      mono: true,
    });
  }

  return {
    advanced,
    protocol: interfaceLabel || null,
    bindingRows: buildTicketBindingRows(bindings, t),
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
    routeLabel: bindingDashboardRouteLabel(binding.route, t),
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
