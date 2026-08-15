/** Thin façade for the ticket / binding read model. */
import { getBackend } from '@/app/runtime';
import type { AdapterApplyPlan, TicketWallet } from '@/lib/backend/contracts';
import type { AgentId } from '@/lib/types';

export type {
  BindingBridgeRuntime,
  BindingRoute,
  BindingView,
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
  ticketCredentialClassLabel,
  ticketSurfaceLabel,
} from '@/lib/backend/contracts/ticket';

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
