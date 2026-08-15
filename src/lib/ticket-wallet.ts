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
