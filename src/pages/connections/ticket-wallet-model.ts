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

export function formatTicketUsageParts(bindings: readonly BindingView[]): TicketUsagePart[] {
  const active = bindings.filter((b) => b.active);
  if (active.length === 0) return [{ kind: 'text', text: '未使用' }];
  const parts: TicketUsagePart[] = [{ kind: 'text', text: '正用于：' }];
  active.forEach((binding, index) => {
    if (index > 0) parts.push({ kind: 'text', text: ' · ' });
    parts.push(...formatBindingUsageParts(binding));
  });
  return parts;
}

export function formatTicketUsageText(bindings: readonly BindingView[]): string {
  return formatTicketUsageParts(bindings)
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
      usageText: formatTicketUsageText(bindings),
      usageParts: formatTicketUsageParts(bindings),
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

/** Read-only fields for the ticket detail expand panel. */
export function buildTicketDetailFields(
  ticket: TicketView,
  extras?: TicketDetailExtras | null,
): TicketDetailField[] {
  const fields: TicketDetailField[] = [
    { label: '类型', value: ticketCredentialClassLabel(ticket.credentialClass) },
    { label: '票面', value: ticketSurfaceLabel(ticket.surface) },
    { label: '所属', value: agentDisplayName(ticket.agentId) },
  ];
  if (ticket.importedFrom) {
    fields.push({ label: '导入自', value: agentDisplayName(ticket.importedFrom) });
  }
  if (extras?.endpointMode) {
    fields.push({
      label: '端点',
      value: extras.endpointMode === 'official' ? '官方' : '自定义',
    });
  }
  if (extras?.identity) {
    fields.push({
      label: ticket.credentialClass === 'oauth' ? '官方账号' : '账号',
      value: extras.identity,
    });
  }
  if (extras?.accountProvider) {
    fields.push({ label: '提供商', value: extras.accountProvider });
  }
  if (extras?.endpointHost) {
    fields.push({ label: 'Endpoint', value: extras.endpointHost, mono: true });
  }
  if (ticket.speaks.length > 0) {
    fields.push({ label: '协议', value: ticket.speaks.join(' · ') });
  }
  return fields;
}

export function formatTicketBindingDetailLines(
  bindings: readonly BindingView[],
): string[] {
  return bindings.map((binding) => {
    const bits = [agentDisplayName(binding.agentId), bindingRouteUsageLabel(binding.route)];
    if (binding.active) bits.push('当前');
    if (binding.route === 'bridge' && binding.bridge?.running) bits.push('运行中');
    if (binding.route === 'bridge' && binding.bridge && !binding.bridge.running) bits.push('已停止');
    if (binding.route === 'bridge' && binding.bridge?.port) {
      bits.push(`端口 ${binding.bridge.port}`);
    }
    return bits.join(' · ');
  });
}

export function ticketDetailEditLabel(extras?: TicketDetailExtras | null): string | null {
  if (extras?.canEditConfig) return '编辑配置';
  if (extras?.canEditKey) return '编辑密钥';
  return null;
}
