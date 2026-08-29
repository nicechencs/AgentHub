/**
 * Ticket wallet filters and exclusion helpers (Connections page).
 */
import type { TranslateFn } from '@/lib/i18n';
import type {
  TicketCredentialClass,
  TicketView,
  TicketWallet,
} from '@/lib/backend/contracts/ticket';
import {
  activeBindingForAgent,
  filterTicketsByAgentUsage,
  filterTicketsByOwner,
} from '@/lib/ticket-wallet';

export { activeBindingForAgent, filterTicketsByAgentUsage, filterTicketsByOwner };

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
  { value: 'all', label: 'All' },
  { value: 'oauth', label: 'Official login' },
  { value: 'api_key', label: 'API Key' },
  { value: 'unknown', label: 'Unrecognized' },
];

export function ticketWalletFilterLabel(
  filter: TicketWalletFilter,
  t?: TranslateFn,
): string {
  if (!t) {
    return TICKET_WALLET_FILTERS.find((item) => item.value === filter)?.label ?? 'All';
  }
  if (filter === 'all') return t('kind.all');
  if (filter === 'oauth') return t('kind.oauth');
  if (filter === 'api_key') return t('kind.apikey');
  return t('connections.list.unrecognized');
}

export function isUnrecognizedTicket(ticket: Pick<TicketView, 'surface' | 'credentialClass'>): boolean {
  return ticket.surface === 'unknown' || ticket.credentialClass === 'unknown';
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
