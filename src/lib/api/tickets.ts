/** Thin façade for the ticket / binding read model and bind/unbind write. */
import { getBackend, notifyBridgePresenceChanged, notifyConnectionPoolChanged } from '@/app/runtime';
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
  ticketCredentialClassLabel,
  ticketIdFor,
  ticketSurfaceLabel,
} from '@/lib/backend/contracts/ticket';

async function refreshConnectionPoolAfterTicketMutation(): Promise<void> {
  try {
    await notifyConnectionPoolChanged(getBackend());
  } catch {
    // The mutation itself succeeded. The pool store keeps previous rows and
    // exposes the refresh error instead of pretending the list is current.
  }
  notifyBridgePresenceChanged();
}

/** Load the global ticket wallet (tickets + bindings). */
export async function listTicketWallet(): Promise<TicketWallet> {
  return getBackend().ticket.listWallet();
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
  await refreshConnectionPoolAfterTicketMutation();
  return result;
}

/** Unbind ticket from agent. Ticket remains; caller may listWallet. */
export async function unbindTicket(ticketId: string, agentId: AgentId): Promise<void> {
  await getBackend().ticket.unbind(ticketId, agentId);
  await refreshConnectionPoolAfterTicketMutation();
}
