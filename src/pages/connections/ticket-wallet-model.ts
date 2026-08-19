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

export function formatBindingUsageParts(binding: BindingView): TicketUsagePart[] {
  const route = bindingRouteUsageLabel(binding.route);
  const name = agentDisplayName(binding.agentId);
  if (binding.route === 'bridge') {
    const suffix = binding.bridge?.running
      ? ' · 运行中'
      : binding.bridge && !binding.bridge.running
        ? ' · 已停止'
        : '';
    return [
      { kind: 'text', text: `${name}（` },
      { kind: 'bridge', label: route, href: bridgesHrefForProfile(binding.profileId) },
      { kind: 'text', text: `${suffix}）` },
    ];
  }
  return [{ kind: 'text', text: `${name}（${route}）` }];
}

export function formatBindingUsagePart(binding: BindingView): string {
  return formatBindingUsageParts(binding)
    .map((part) => (part.kind === 'bridge' ? part.label : part.text))
    .join('');
}

export function formatTicketUsageParts(
  bindings: readonly BindingView[],
  ownerAgentId?: AgentId,
): TicketUsagePart[] {
  const active = bindings.filter((b) => b.active);
  if (active.length === 0) {
    return [{
      kind: 'text',
      text: ownerAgentId ? `${agentDisplayName(ownerAgentId)} · 未使用` : '未使用',
    }];
  }
  const selfOnly =
    Boolean(ownerAgentId)
    && active.length === 1
    && active[0]!.agentId === ownerAgentId;
  if (selfOnly) {
    return formatBindingUsageParts(active[0]!);
  }
  const parts: TicketUsagePart[] = [{ kind: 'text', text: '正用于：' }];
  active.forEach((binding, index) => {
    if (index > 0) parts.push({ kind: 'text', text: ' · ' });
    parts.push(...formatBindingUsageParts(binding));
  });
  return parts;
}

export function formatTicketUsageText(
  bindings: readonly BindingView[],
  ownerAgentId?: AgentId,
): string {
  return formatTicketUsageParts(bindings, ownerAgentId)
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
    ...(ticket.speaks ?? []),
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
  } = {},
): TicketWalletRow[] {
  const filter = options.filter ?? 'all';
  const query = options.query ?? '';
  const highlightAgentId = options.highlightAgentId ?? null;
  const agentFilterId = options.agentFilterId ?? null;

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
      usageText: formatTicketUsageText(bindings, ticket.agentId),
      usageParts: formatTicketUsageParts(bindings, ticket.agentId),
    };
  });
}

export function dashboardBindingMetaText(
  ticketLabel: string,
  route: BindingRoute,
): string {
  return `${ticketLabel} · ${bindingRouteDashboardLabel(route)}`;
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
  '可续期·未验证': '可续期，尚未验证',
  '已配置·未验证': '已配置，尚未验证',
};

/** Human words for login health; never dump「可续期·未验证」as a raw token. */
export function humanizeTicketAuthLabel(label: string): string {
  return AUTH_LABEL_HUMAN[label] ?? label.replaceAll('·', '，');
}

function endpointHostOnly(host: string): string {
  try {
    if (/^https?:\/\//i.test(host)) return new URL(host).host;
  } catch {
    /* keep raw host */
  }
  return host;
}

export function ticketBindingStatus(binding: BindingView): string {
  if (binding.route === 'bridge') {
    if (binding.bridge?.running) return '本机路由运行中';
    if (binding.bridge && !binding.bridge.running) return '本机路由已停止';
  }
  return binding.active ? '当前使用' : '未使用';
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
          ?? '官方未提供账号信息'
        : source.account.email ?? source.account.identityLabel ?? source.account.label;
    if (source.account.provider && !ticket.label.includes(source.account.provider)) {
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
 * Header already shows 类型 / 来源 / 所属 / 官方账号 / email / 订阅.
 */
export function buildTicketDetailFields(
  ticket: TicketView,
  extras?: TicketDetailExtras | null,
): TicketDetailSections {
  const advanced: TicketDetailField[] = [];
  if (ticket.importedFrom) {
    advanced.push({ label: '导入自', value: agentDisplayName(ticket.importedFrom) });
  }
  if (extras?.authLabel) {
    advanced.push({
      label: '登录状态',
      value: humanizeTicketAuthLabel(extras.authLabel),
    });
  }

  const customEndpoint = extras != null && extras.endpointMode === 'custom';
  if (customEndpoint) {
    advanced.push({ label: '端点', value: '自定义' });
    if (extras.endpointHost) {
      advanced.push({
        label: 'Endpoint',
        value: endpointHostOnly(extras.endpointHost),
        mono: true,
      });
    }
  }

  const showProtocol =
    ticket.speaks.length > 0
    && (ticket.credentialClass === 'api_key' || customEndpoint);
  if (showProtocol) {
    advanced.push({ label: '协议', value: ticket.speaks.join(' · ') });
  }

  return { advanced };
}

export function formatTicketBindingDetailLines(
  bindings: readonly BindingView[],
): TicketBindingDetailLine[] {
  return bindings.map((binding) => ({
    agent: agentDisplayName(binding.agentId),
    status: ticketBindingStatus(binding),
  }));
}

export function ticketDetailEditLabel(extras?: TicketDetailExtras | null): string | null {
  if (extras?.canEditConfig) return '编辑配置';
  if (extras?.canEditKey) return '编辑密钥';
  return null;
}
