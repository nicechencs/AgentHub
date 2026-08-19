/**
 * Global ticket-wallet list helpers (Connections page).
 * Filter / search / binding usage lines — pure functions for vitest.
 */
import { agentDisplayName } from '@/config/agents';
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
  bindingRouteUsageLabel,
  ticketCredentialClassLabel,
  ticketSurfaceLabel,
} from '@/lib/backend/contracts/ticket';
import {
  providerEndpointExtras,
  toCredentialRow,
} from '@/lib/credential-row';
import { bridgesHrefForProfile } from '@/lib/bridges-path';
import type { TranslateFn } from '@/lib/i18n';

export { activeBindingForAgent } from '@/lib/ticket-wallet';

export type TicketWalletFilter = 'all' | TicketCredentialClass;

export const TICKET_WALLET_FILTERS: Array<{ value: TicketWalletFilter; label: string }> = [
  { value: 'all', label: '全部' },
  { value: 'oauth', label: '官方登录' },
  { value: 'api_key', label: 'API Key' },
  { value: 'unknown', label: '未识别' },
];

export type TicketAddKind = 'import-login' | 'api-key';

export const TICKET_ADD_ACTIONS: Array<{ kind: TicketAddKind; label: string }> = [
  { kind: 'import-login', label: '导入当前登录' },
  { kind: 'api-key', label: '添加 API Key' },
];

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
  return kind === 'import-login'
    ? t('connections.list.importLogin')
    : t('connections.list.addApiKey');
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

function bindingUsageRouteLabel(route: BindingRoute, t?: TranslateFn): string {
  if (!t) return bindingRouteUsageLabel(route);
  if (route === 'reshape') return t('connections.list.routeReshape');
  if (route === 'bridge') return t('kind.route.localRoute');
  return t('connections.list.routeSwitch');
}

function bindingDashboardRouteLabel(route: BindingRoute, t?: TranslateFn): string {
  if (!t) return bindingRouteDashboardLabel(route);
  if (route === 'reshape') return t('connections.list.routeReshape');
  if (route === 'bridge') return t('kind.route.localRoute');
  return t('kind.route.direct');
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
    actions: TICKET_ADD_ACTIONS,
  }));
}

export function dispatchTicketAddAction(
  kind: TicketAddKind,
  agentId: AgentId,
  handlers: {
    onImportLogin?: (id: AgentId) => void;
    onAddKey?: (id: AgentId) => void;
  },
): void {
  if (kind === 'import-login') {
    handlers.onImportLogin?.(agentId);
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
 */
export function handleTicketAddMenuSelect(
  event: { preventDefault: () => void },
  kind: TicketAddKind,
  agentId: AgentId,
  handlers: {
    onImportLogin?: (id: AgentId) => void;
    onAddKey?: (id: AgentId) => void;
    onMenuClose?: () => void;
  },
  schedule: (fn: () => void) => void = scheduleAfterMenuClose,
): void {
  event.preventDefault();
  dispatchTicketAddAction(kind, agentId, handlers);
  if (handlers.onMenuClose) schedule(handlers.onMenuClose);
}

export function ticketAddDialogState(
  kind: TicketAddKind,
  agentId: AgentId,
): {
  addAgentId: AgentId;
  loginImportOpen: boolean;
  apiKeyDialogOpen: boolean;
  clearEditProvider: boolean;
} {
  return {
    addAgentId: agentId,
    loginImportOpen: kind === 'import-login',
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
  | { kind: 'bridge'; label: string; href: string };

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
): TicketUsagePart[] {
  const route = bindingUsageRouteLabel(binding.route, t);
  const name = agentDisplayName(binding.agentId);
  if (binding.route === 'bridge') {
    const suffix = binding.bridge?.running
      ? (t ? t('connections.list.runningSuffix') : ' · 运行中')
      : binding.bridge && !binding.bridge.running
        ? (t ? t('connections.list.stoppedSuffix') : ' · 已停止')
        : '';
    return [
      { kind: 'text', text: t ? t('connections.list.usageOpen', { name }) : `${name}（` },
      { kind: 'bridge', label: route, href: bridgesHrefForProfile(binding.profileId) },
      { kind: 'text', text: t ? t('connections.list.usageCloseWithSuffix', { suffix }) : `${suffix}）` },
    ];
  }
  return [{
    kind: 'text',
    text: t ? t('connections.list.usageWithRoute', { name, route }) : `${name}（${route}）`,
  }];
}

export function formatBindingUsagePart(binding: BindingView, t?: TranslateFn): string {
  return formatBindingUsageParts(binding, t)
    .map((part) => (part.kind === 'bridge' ? part.label : part.text))
    .join('');
}

export function formatTicketUsageParts(
  bindings: readonly BindingView[],
  ownerAgentId?: AgentId,
  t?: TranslateFn,
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
    return formatBindingUsageParts(active[0]!, t);
  }
  const parts: TicketUsagePart[] = [{
    kind: 'text',
    text: t ? t('connections.list.usedFor') : '正用于：',
  }];
  active.forEach((binding, index) => {
    if (index > 0) parts.push({ kind: 'text', text: ' · ' });
    parts.push(...formatBindingUsageParts(binding, t));
  });
  return parts;
}

export function formatTicketUsageText(
  bindings: readonly BindingView[],
  ownerAgentId?: AgentId,
  t?: TranslateFn,
): string {
  return formatTicketUsageParts(bindings, ownerAgentId, t)
    .map((part) => (part.kind === 'bridge' ? part.label : part.text))
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

function ticketSearchHaystack(
  ticket: TicketView,
  bindings: readonly BindingView[],
): string {
  const own = bindings.filter((binding) => binding.ticketId === ticket.id);
  const usageText = formatTicketUsageText(own);
  const bindingBits = own.flatMap((binding) => [
    binding.agentId,
    agentDisplayName(binding.agentId),
    bindingRouteUsageLabel(binding.route),
    bindingRouteDashboardLabel(binding.route),
  ]);
  return [
    ticket.label,
    ticket.id,
    ticket.agentId,
    ticket.surface,
    ticket.credentialClass,
    ticketCredentialClassLabel(ticket.credentialClass),
    ticketSurfaceLabel(ticket.surface),
    agentDisplayName(ticket.agentId),
    ...(Array.isArray(ticket.speaks) ? ticket.speaks : []),
    usageText,
    ...bindingBits,
  ]
    .join(' ')
    .toLowerCase();
}

/** Matches ticket fields and「正用于」bindings (agent / route label / usageText). */
export function searchTickets(
  tickets: readonly TicketView[],
  query: string,
  bindings: readonly BindingView[] = [],
): TicketView[] {
  const q = query.trim().toLowerCase();
  if (!q) return [...tickets];
  return tickets.filter((ticket) => ticketSearchHaystack(ticket, bindings).includes(q));
}

/** Soft agent filter: tickets that belong to or bind to the agent. */
export function filterTicketsByAgentUsage(
  wallet: TicketWallet,
  tickets: readonly TicketView[],
  agentId: AgentId | null,
): TicketView[] {
  if (!agentId) return [...tickets];
  const ticketIds = new Set(
    wallet.bindings.filter((b) => b.agentId === agentId).map((b) => b.ticketId),
  );
  return tickets.filter((t) => ticketIds.has(t.id) || t.agentId === agentId);
}

export function buildTicketWalletRows(
  wallet: TicketWallet,
  options: {
    filter?: TicketWalletFilter;
    query?: string;
    /** Deep-link agent: highlight active binding rows; does not privatize the list. */
    highlightAgentId?: AgentId | null;
    /** Optional soft filter by agent (UI chip); omit for full wallet. */
    agentFilterId?: AgentId | null;
    t?: TranslateFn;
  } = {},
): TicketWalletRow[] {
  const filter = options.filter ?? 'all';
  const query = options.query ?? '';
  const highlightAgentId = options.highlightAgentId ?? null;
  const agentFilterId = options.agentFilterId ?? null;
  const t = options.t;

  let tickets = filterTickets(wallet.tickets, filter);
  tickets = searchTickets(tickets, query, wallet.bindings);
  if (agentFilterId) {
    tickets = filterTicketsByAgentUsage(wallet, tickets, agentFilterId);
  }

  return tickets.map((ticket) => {
    const bindings = bindingsForTicket(wallet, ticket.id);
    const highlighted = Boolean(
      highlightAgentId
      && bindings.some((b) => b.active && b.agentId === highlightAgentId),
    );
    return {
      ticket,
      bindings,
      highlighted,
      usageText: formatTicketUsageText(bindings, ticket.agentId, t),
      usageParts: formatTicketUsageParts(bindings, ticket.agentId, t),
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
}

export interface TicketDetailField {
  label: string;
  value: string;
  mono?: boolean;
}

export interface TicketDetailSections {
  /** Non-duplicate facts for the collapsed「更多」block. */
  advanced: TicketDetailField[];
}

export interface TicketBindingDetailLine {
  agent: string;
  status: string;
}

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
): TicketDetailExtras {
  const extras: TicketDetailExtras = {
    canEditKey: ticket.sourceKind === 'account' && source.account?.kind === 'apikey',
    canEditConfig: ticket.sourceKind === 'provider' && Boolean(source.provider),
    isCurrent: source.account?.isCurrent === true || source.provider?.isCurrent === true,
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
  }

  if (source.provider) {
    const row = toCredentialRow({ source: 'provider', provider: source.provider });
    const endpoint = providerEndpointExtras(source.provider);
    extras.endpointMode = endpoint.endpointMode;
    extras.endpointHost = endpoint.endpointHost;
    extras.authLabel = row.auth.label;
    extras.authStatus = row.auth.status;
  }

  return extras;
}

/**
 * Advanced-only facts for the ticket detail expand.
 * Header already shows type / surface / health chip; footer shows 导入自.
 */
export function buildTicketDetailFields(
  ticket: TicketView,
  extras?: TicketDetailExtras | null,
  t?: TranslateFn,
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
        label: 'Endpoint',
        value: endpointHostOnly(extras.endpointHost),
        mono: true,
      });
    }
  }

  const speaks = Array.isArray(ticket.speaks) ? ticket.speaks : [];
  const showProtocol =
    speaks.length > 0
    && (ticket.credentialClass === 'api_key' || customEndpoint);
  if (showProtocol) {
    advanced.push({
      label: t ? t('connections.list.protocol') : '协议',
      value: speaks.join(' · '),
    });
  }

  return { advanced };
}

export function formatTicketBindingDetailLines(
  bindings: readonly BindingView[],
  t?: TranslateFn,
): TicketBindingDetailLine[] {
  return bindings.map((binding) => ({
    agent: agentDisplayName(binding.agentId),
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
