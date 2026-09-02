/**
 * Ticket binding usage lines and wallet row assembly (Connections page).
 */
import { agentDisplayName } from '@/config/agents';
import {
  formatRouteEndpointHttpUrl,
  routeEndpointIdForBinding,
  routeEndpointPathForBinding,
  type RouteEndpointId,
} from '@/lib/route-endpoints';
import type { AgentKey } from '@/lib/types';
import type {
  BindingRoute,
  BindingView,
  TicketView,
  TicketWallet,
} from '@/lib/backend/contracts/ticket';
import { surfaceGroupMemberCount } from '@/lib/backend/contracts/ticket';
import { routesHrefForProfile } from '@/lib/routes-path';
import type { TranslateFn } from '@/lib/i18n';
import { filterTicketsByOwner } from '@/lib/ticket-wallet';
import { connectionStateRouteLabel } from '@/lib/ticket-wallet-labels';
import {
  filterTickets,
  type TicketWalletFilter,
} from './ticket-wallet-filters';

export { connectionStateRouteLabel } from '@/lib/ticket-wallet-labels';

function bindingUsageRouteLabel(route: BindingRoute, t?: TranslateFn): string {
  return connectionStateRouteLabel(route, t);
}

function bindingDashboardRouteLabel(route: BindingRoute, t?: TranslateFn): string {
  return connectionStateRouteLabel(route, t);
}

export type TicketUsagePart =
  | { kind: 'text'; text: string }
  | { kind: 'bridge'; label: string; href: string }
  | { kind: 'endpoint'; path: string; port: number | null; endpointId: RouteEndpointId };

export interface TicketWalletRow {
  ticket: TicketView;
  bindings: BindingView[];
  /** Active bindings for highlightAgent (deep-link ?agent=). */
  highlighted: boolean;
  usageText: string;
  usageParts: TicketUsagePart[];
}

export function bindingsForTicket(
  wallet: TicketWallet,
  ticketId: string,
): BindingView[] {
  return wallet.bindings.filter((b) => b.ticketId === ticketId);
}

export function formatBindingUsageParts(
  binding: BindingView,
  t?: TranslateFn,
  memberCount = 1,
): TicketUsagePart[] {
  const route = bindingUsageRouteLabel(binding.route, t);
  const name = agentDisplayName(binding.agentId);
  const poolSuffix = binding.route === 'bridge' && memberCount > 1
    ? (t
      ? t('connections.list.poolSuffix', { n: memberCount })
      : ` · ${memberCount} 份同类登录可轮换`)
    : '';
  if (binding.route === 'native') {
    return [{ kind: 'text', text: name }];
  }
  if (binding.route === 'bridge') {
    const path = routeEndpointPathForBinding({ agentId: binding.agentId });
    const endpointId = routeEndpointIdForBinding({ agentId: binding.agentId });
    const port = binding.bridge?.running ? binding.bridge.port ?? null : null;
    const suffix = binding.bridge?.running
      ? `${poolSuffix}${t ? t('connections.list.runningSuffix') : ' · 运行中'}`
      : binding.bridge && !binding.bridge.running
        ? `${poolSuffix}${t ? t('connections.list.stoppedSuffix') : ' · 已停止'}`
        : poolSuffix;
    return [
      { kind: 'endpoint', path, port, endpointId },
      { kind: 'text', text: '（' },
      { kind: 'bridge', label: route, href: routesHrefForProfile(binding.profileId) },
      { kind: 'text', text: t ? t('connections.list.usageCloseWithSuffix', { suffix }) : `${suffix}）` },
    ];
  }
  return [{
    kind: 'text',
    text: t ? t('connections.list.usageWithRoute', { name, route }) : `${name}（${route}）`,
  }];
}

export function formatBindingUsagePart(
  binding: BindingView,
  t?: TranslateFn,
  memberCount = 1,
): string {
  return formatBindingUsageParts(binding, t, memberCount)
    .map((part) => usagePartPlainText(part))
    .join('');
}

function usagePartPlainText(part: TicketUsagePart): string {
  if (part.kind === 'bridge') return part.label;
  if (part.kind === 'endpoint') {
    return formatRouteEndpointHttpUrl({ path: part.path, port: part.port });
  }
  return part.text;
}

export function formatTicketUsageParts(
  bindings: readonly BindingView[],
  ownerAgentId?: AgentKey,
  t?: TranslateFn,
  memberCount = 1,
  isCurrent = false,
): TicketUsagePart[] {
  const active = bindings.filter((b) => b.active);
  if (active.length === 0) {
    if (isCurrent) {
      return [{
        kind: 'text',
        text: ownerAgentId
          ? (t
            ? t('connections.list.inUseWithOwner', { name: agentDisplayName(ownerAgentId) })
            : `${agentDisplayName(ownerAgentId)} · 使用中`)
          : (t ? t('connections.list.inUse') : '使用中'),
      }];
    }
    return [{
      kind: 'text',
      text: ownerAgentId
        ? (t
          ? t('connections.list.unusedWithOwner', { name: agentDisplayName(ownerAgentId) })
          : `${agentDisplayName(ownerAgentId)} · 未使用`)
        : (t ? t('connections.list.unused') : '未使用'),
    }];
  }
  const selfOnly =
    Boolean(ownerAgentId)
    && active.length === 1
    && active[0]!.agentId === ownerAgentId;
  if (selfOnly) {
    return formatBindingUsageParts(active[0]!, t, memberCount);
  }
  const parts: TicketUsagePart[] = [{
    kind: 'text',
    text: t ? t('connections.list.usedFor') : '正用于：',
  }];
  active.forEach((binding, index) => {
    if (index > 0) parts.push({ kind: 'text', text: ' · ' });
    parts.push(...formatBindingUsageParts(binding, t, memberCount));
  });
  return parts;
}

export function formatTicketUsageText(
  bindings: readonly BindingView[],
  ownerAgentId?: AgentKey,
  t?: TranslateFn,
  memberCount = 1,
  isCurrent = false,
): string {
  return formatTicketUsageParts(bindings, ownerAgentId, t, memberCount, isCurrent)
    .map((part) => usagePartPlainText(part))
    .join('');
}

export function buildTicketWalletRows(
  wallet: TicketWallet,
  options: {
    filter?: TicketWalletFilter;
    /** Deep-link highlight for that Agent's active binding. */
    highlightAgentId?: AgentKey | null;
    /** Agent tab filter; omit for the full wallet. */
    agentFilterId?: AgentKey | null;
    t?: TranslateFn;
    /** True when this ticket is the current login for its agent (even with no active bindings). */
    isCurrentForTicket?: (ticket: TicketView) => boolean;
  } = {},
): TicketWalletRow[] {
  const filter = options.filter ?? 'all';
  const highlightAgentId = options.highlightAgentId ?? null;
  const agentFilterId = options.agentFilterId ?? null;
  const t = options.t;
  const isCurrentForTicket = options.isCurrentForTicket;

  let tickets = filterTickets(wallet.tickets, filter);
  if (agentFilterId) {
    tickets = filterTicketsByOwner(tickets, agentFilterId);
  }

  return tickets.map((ticket) => {
    const bindings = bindingsForTicket(wallet, ticket.id);
    const memberCount = surfaceGroupMemberCount(wallet.surfaceGroups, ticket.id);
    const highlighted = Boolean(
      highlightAgentId
      && bindings.some((b) => b.active && b.agentId === highlightAgentId),
    );
    const isCurrent = isCurrentForTicket?.(ticket) === true;
    return {
      ticket,
      bindings,
      highlighted,
      usageText: formatTicketUsageText(bindings, ticket.agentId, t, memberCount, isCurrent),
      usageParts: formatTicketUsageParts(bindings, ticket.agentId, t, memberCount, isCurrent),
    };
  });
}

export function dashboardBindingMetaText(
  ticketLabel: string,
  route: BindingRoute,
  t?: TranslateFn,
  localUrl?: string | null,
): string {
  if (route === 'bridge') {
    const url = localUrl?.trim();
    return url ? `${ticketLabel} · ${url}` : ticketLabel;
  }
  const routeLabel = bindingDashboardRouteLabel(route, t);
  return routeLabel ? `${ticketLabel} · ${routeLabel}` : ticketLabel;
}
