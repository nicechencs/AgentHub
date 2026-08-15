/**
 * Global ticket-wallet list helpers (Connections page).
 * Filter / search / binding usage lines — pure functions for vitest.
 */
import { agentDisplayName } from '@/config/agents';
import type { AgentId } from '@/lib/types';
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

export type TicketWalletFilter = 'all' | TicketCredentialClass;

export const TICKET_WALLET_FILTERS: Array<{ value: TicketWalletFilter; label: string }> = [
  { value: 'all', label: '全部' },
  { value: 'oauth', label: '官方登录' },
  { value: 'api_key', label: 'API Key' },
  { value: 'unknown', label: '未识别' },
];

export interface TicketWalletRow {
  ticket: TicketView;
  bindings: BindingView[];
  /** Active bindings for highlightAgent (deep-link ?agent=). */
  highlighted: boolean;
  usageText: string;
}

export function bindingsForTicket(
  wallet: TicketWallet,
  ticketId: string,
): BindingView[] {
  return wallet.bindings.filter((b) => b.ticketId === ticketId);
}

export function formatBindingUsagePart(binding: BindingView): string {
  const route = bindingRouteUsageLabel(binding.route);
  const name = agentDisplayName(binding.agentId);
  if (binding.route === 'bridge' && binding.bridge?.running) {
    return `${name}（${route} · 运行中）`;
  }
  if (binding.route === 'bridge' && binding.bridge && !binding.bridge.running) {
    return `${name}（${route} · 已停止）`;
  }
  return `${name}（${route}）`;
}

export function formatTicketUsageText(bindings: readonly BindingView[]): string {
  const active = bindings.filter((b) => b.active);
  if (active.length === 0) return '未使用';
  return `正用于：${active.map(formatBindingUsagePart).join(' · ')}`;
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
    counts[ticket.credentialClass] += 1;
  }
  return counts;
}

export function filterTickets(
  tickets: readonly TicketView[],
  filter: TicketWalletFilter,
): TicketView[] {
  if (filter === 'all') return [...tickets];
  return tickets.filter((t) => t.credentialClass === filter);
}

export function searchTickets(
  tickets: readonly TicketView[],
  query: string,
): TicketView[] {
  const q = query.trim().toLowerCase();
  if (!q) return [...tickets];
  return tickets.filter((ticket) => {
    const hay = [
      ticket.label,
      ticket.id,
      ticket.agentId,
      ticket.surface,
      ticket.credentialClass,
      ticketCredentialClassLabel(ticket.credentialClass),
      ticketSurfaceLabel(ticket.surface),
      agentDisplayName(ticket.agentId),
      ...(ticket.speaks ?? []),
    ]
      .join(' ')
      .toLowerCase();
    return hay.includes(q);
  });
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
  tickets = searchTickets(tickets, query);
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
    };
  });
}

/** Active binding for a Dashboard agent card. */
export function activeBindingForAgent(
  wallet: TicketWallet,
  agentId: AgentId,
): { ticket: TicketView; binding: BindingView } | null {
  const binding = wallet.bindings.find((b) => b.agentId === agentId && b.active);
  if (!binding) return null;
  const ticket = wallet.tickets.find((t) => t.id === binding.ticketId);
  if (!ticket) return null;
  return { ticket, binding };
}

export function dashboardBindingMetaText(
  ticketLabel: string,
  route: BindingRoute,
): string {
  return `${ticketLabel} · ${bindingRouteDashboardLabel(route)}`;
}
