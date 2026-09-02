import type { AgentKey } from '@/lib/types';
import type { BindingView, TicketView, TicketWallet } from '@/lib/backend/contracts/ticket';

/** Active TicketBinding for a Dashboard / wallet agent card. Not ActiveBinding. */
export function activeBindingForAgent(
  wallet: TicketWallet,
  agentId: AgentKey,
): { ticket: TicketView; binding: BindingView } | null {
  const binding = wallet.bindings.find((b) => b.agentId === agentId && b.active);
  if (!binding) return null;
  const ticket = wallet.tickets.find((t) => t.id === binding.ticketId);
  if (!ticket) return null;
  return { ticket, binding };
}

/** Agent filter: tickets owned by the agent or with an active binding to it. */
export function filterTicketsByAgentUsage(
  wallet: TicketWallet,
  tickets: readonly TicketView[],
  agentId: AgentKey | null,
): TicketView[] {
  if (!agentId) return [...tickets];
  const ticketIds = new Set(
    wallet.bindings
      .filter((b) => b.agentId === agentId && b.active)
      .map((b) => b.ticketId),
  );
  return tickets.filter((t) => ticketIds.has(t.id) || t.agentId === agentId);
}

/** Connections tab chips: each login counts once, under its owner agent. */
export function filterTicketsByOwner(
  tickets: readonly TicketView[],
  agentId: AgentKey | null,
): TicketView[] {
  if (!agentId) return [...tickets];
  return tickets.filter((ticket) => ticket.agentId === agentId);
}
