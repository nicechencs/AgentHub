import type {
  AdapterApplyPlan,
  BindTicketResult,
  TicketPort,
  TicketWallet,
} from '@/lib/backend/contracts';
import {
  mapBindTicketResult,
  mapPlanTicketResult,
  mapTicketWallet,
  mapUnbindTicketResult,
  type BindTicketResultWire,
  type PlanTicketResultWire,
  type TicketWalletWire,
} from '@/lib/backend/contracts/ticket';
import type { AgentId } from '@/lib/types';
import { mapAdapterInvokeError } from './adapter';
import { invoke } from './invoke';

async function invokeTicket<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (error) {
    mapAdapterInvokeError(error);
  }
}

/** Tauri ticket-wallet + plan/bind/unbind transport. */
export function createTauriTicketPort(): TicketPort {
  return {
    async listWallet(): Promise<TicketWallet> {
      const wire = await invokeTicket<TicketWalletWire>('list_ticket_wallet');
      return mapTicketWallet(wire);
    },
    async plan(ticketId: string, targetAgentId: AgentId): Promise<AdapterApplyPlan> {
      const wire = await invokeTicket<PlanTicketResultWire>('plan_ticket', {
        ticketId,
        targetAgentId,
      });
      return mapPlanTicketResult(wire);
    },
    async bind(ticketId: string, targetAgentId: AgentId): Promise<BindTicketResult> {
      const wire = await invokeTicket<BindTicketResultWire>('bind_ticket', {
        ticketId,
        targetAgentId,
      });
      return mapBindTicketResult(wire);
    },
    async unbind(ticketId: string, agentId: AgentId): Promise<void> {
      const wire = await invokeTicket<unknown>('unbind_ticket', {
        ticketId,
        agentId,
      });
      mapUnbindTicketResult(wire);
    },
  };
}
