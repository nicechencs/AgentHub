/** Thin façade for the ticket / binding read model and bind/unbind write. */
import {
  getBackend,
  loadTicketWallet,
  notifyConnectionPoolChanged,
  notifyTicketWalletChanged,
} from '@/app/runtime';
import type {
  AdapterApplyPlan,
  BindTicketResult,
  TicketWallet,
} from '@/lib/backend/contracts';
import type { AgentId } from '@/lib/types';

export type {
  BindingBridgeRuntime,
  BindingRoute,
  BindingView,
  BindTicketResult,
  TicketCredentialClass,
  TicketPort,
  TicketSourceKind,
  TicketSurface,
  TicketView,
  TicketWallet,
} from '@/lib/backend/contracts/ticket';

export {
  bindingRouteDashboardLabel,
  bindingRouteUsageLabel,
  isActiveBindingForAgent,
  isBindSuccessForAgent,
  ticketCredentialClassLabel,
  ticketIdFor,
  ticketSurfaceLabel,
} from '@/lib/backend/contracts/ticket';

async function refreshAfterTicketMutation(): Promise<void> {
  const backend = getBackend();
  await Promise.all([
    notifyConnectionPoolChanged(backend).catch(() => {
      // The mutation itself succeeded. The pool store keeps previous rows.
    }),
    notifyTicketWalletChanged(backend).catch(() => {
      // Same: keep the last good wallet and let the next page read retry.
    }),
  ]);
}

/** Load the global ticket wallet (tickets + bindings). Shared process cache. */
export async function listTicketWallet(): Promise<TicketWallet> {
  const snapshot = await loadTicketWallet(getBackend());
  if (snapshot.wallet) return snapshot.wallet;
  throw snapshot.error ?? new Error('ticket wallet unavailable');
}

/** Plan bind(ticket, agent); same shape as planAdapter. */
export async function planTicket(
  ticketId: string,
  targetAgentId: AgentId,
): Promise<AdapterApplyPlan> {
  return getBackend().ticket.plan(ticketId, targetAgentId);
}

/** Bind ticket → agent. Success is the returned active binding. */
export async function bindTicket(
  ticketId: string,
  targetAgentId: AgentId,
): Promise<BindTicketResult> {
  const result = await getBackend().ticket.bind(ticketId, targetAgentId);
  await refreshAfterTicketMutation();
  return result;
}

/** Unbind ticket from agent. Ticket remains; caller may listWallet. */
export async function unbindTicket(ticketId: string, agentId: AgentId): Promise<void> {
  await getBackend().ticket.unbind(ticketId, agentId);
  await refreshAfterTicketMutation();
}
