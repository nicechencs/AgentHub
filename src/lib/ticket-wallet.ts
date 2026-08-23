import type { AgentId } from '@/lib/types';
import type { BindingView, TicketView, TicketWallet } from '@/lib/backend/contracts/ticket';

/** Active TicketBinding for a Dashboard / wallet agent card. Not ActiveBinding. */
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
