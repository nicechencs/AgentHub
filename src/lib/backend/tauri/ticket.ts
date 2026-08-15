import type { AdapterApplyPlan, TicketPort, TicketWallet } from '@/lib/backend/contracts';
import {
  mapPlanTicketResult,
  mapTicketWallet,
  type TicketWalletWire,
} from '@/lib/backend/contracts/ticket';
import type { AdapterApplyPlanWire } from '@/lib/backend/contracts/adapter-wire';
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

/** Tauri ticket-wallet read model + plan_ticket transport. */
export function createTauriTicketPort(): TicketPort {
  return {
    async listWallet(): Promise<TicketWallet> {
      const wire = await invokeTicket<TicketWalletWire>('list_ticket_wallet');
      return mapTicketWallet(wire);
    },
    async plan(ticketId: string, targetAgentId: AgentId): Promise<AdapterApplyPlan> {
      const wire = await invokeTicket<AdapterApplyPlanWire>('plan_ticket', {
        ticketId,
        targetAgentId,
      });
      return mapPlanTicketResult(wire);
    },
  };
}
