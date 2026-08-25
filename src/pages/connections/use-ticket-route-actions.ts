/**
 * Connections row 本机转发 enablement: plan fan-out against catalog agents.
 * Authority is plan.canApply + local_bridge; UI only maps settled hints.
 */
import { useCallback, useEffect, useMemo, useSyncExternalStore } from 'react';
import { AGENT_IDS } from '@/config/agents';
import type { ConnectFlowDeps, PlanEligibility, PlanFanoutRequest } from '@/lib/connect-flow/types';
import { planFanoutKey } from '@/lib/connect-flow/types';
import type { TranslateFn } from '@/lib/i18n';
import type { Account, AgentId, Provider } from '@/lib/types';
import type { TicketView } from '@/lib/backend/contracts/ticket';
import {
  resolveTicketBindAction,
  type TicketBindAction,
  type TicketBindPurpose,
  type TicketRoutePlanHint,
} from './ticket-wallet-model';

const EMPTY_ELIGIBILITY: ReadonlyMap<string, PlanEligibility> = new Map();

function uniqueAgentIds(
  accounts: readonly Account[],
  providers: readonly Provider[],
  tickets: readonly TicketView[],
): AgentId[] {
  const ids = new Set<AgentId>();
  for (const account of accounts) ids.add(account.agentId);
  for (const provider of providers) ids.add(provider.agentId);
  for (const ticket of tickets) ids.add(ticket.agentId);
  return [...ids];
}

export function catalogTargetsForRoute(
  hiddenIds: ReadonlySet<AgentId> | readonly AgentId[],
  accounts: readonly Account[],
  providers: readonly Provider[],
  tickets: readonly TicketView[],
): AgentId[] {
  const hidden = hiddenIds instanceof Set ? hiddenIds : new Set(hiddenIds);
  const ids = AGENT_IDS.length > 0
    ? [...AGENT_IDS]
    : uniqueAgentIds(accounts, providers, tickets);
  return ids.filter((id) => !hidden.has(id));
}

export function ticketRouteHintFromEligibility(
  eligibility: PlanEligibility | undefined,
): TicketRoutePlanHint {
  if (!eligibility || eligibility.kind === 'loading') return { status: 'pending' };
  if (eligibility.kind === 'blocked_oauth') {
    return { status: 'blocked_oauth', reason: eligibility.message };
  }
  if (eligibility.kind === 'error') {
    return { status: 'error', reason: eligibility.message };
  }
  return {
    status: 'ready',
    route: eligibility.plan.analysis.route,
    canApply: eligibility.plan.canApply,
    reason: eligibility.reason ?? eligibility.plan.reason ?? eligibility.plan.analysis.reason,
  };
}

export function useTicketBindActions(input: {
  tickets: readonly TicketView[];
  accounts: readonly Account[];
  providers: readonly Provider[];
  hiddenIds: readonly AgentId[];
  poolReady: boolean;
  deps: Pick<ConnectFlowDeps, 'createPlanFanout'>;
  t: TranslateFn;
}): {
  shareActionForTicket: (ticket: TicketView) => TicketBindAction;
  routeActionForTicket: (ticket: TicketView) => TicketBindAction;
} {
  const { tickets, accounts, providers, hiddenIds, poolReady, deps, t } = input;
  const fanout = useMemo(() => deps.createPlanFanout(), [deps]);

  useEffect(() => () => {
    fanout.cancel();
  }, [fanout]);

  const eligibilities = useSyncExternalStore(
    useCallback((listener) => fanout.subscribe(listener), [fanout]),
    useCallback(() => fanout.getState(), [fanout]),
    () => EMPTY_ELIGIBILITY,
  );

  const targetIds = useMemo(
    () => catalogTargetsForRoute(hiddenIds, accounts, providers, tickets),
    [hiddenIds, accounts, providers, tickets],
  );

  const requests = useMemo<PlanFanoutRequest[]>(() => {
    if (!poolReady || tickets.length === 0 || targetIds.length === 0) return [];
    const out: PlanFanoutRequest[] = [];
    for (const ticket of tickets) {
      for (const targetAgentId of targetIds) {
        out.push({
          source: { kind: ticket.sourceKind, id: ticket.sourceId },
          targetAgentId,
        });
      }
    }
    return out;
  }, [poolReady, tickets, targetIds]);

  useEffect(() => {
    if (requests.length === 0) return;
    fanout.start(requests, { accounts });
  }, [fanout, requests, accounts]);

  const actionFor = useCallback((purpose: TicketBindPurpose) => {
    return (ticket: TicketView): TicketBindAction => {
      if (targetIds.length === 0) return resolveTicketBindAction([], purpose, t);
      const hints = targetIds.map((targetAgentId) => (
        ticketRouteHintFromEligibility(
          eligibilities.get(planFanoutKey({
            source: { kind: ticket.sourceKind, id: ticket.sourceId },
            targetAgentId,
          })),
        )
      ));
      return resolveTicketBindAction(hints, purpose, t);
    };
  }, [eligibilities, t, targetIds]);

  return {
    shareActionForTicket: actionFor('share'),
    routeActionForTicket: actionFor('route'),
  };
}
