/**
 * Ticket bind / share / route plan button resolution (Connections page).
 */
import { bindRouteMatchesPurpose } from '@/lib/connect-flow/types';
import type { TranslateFn } from '@/lib/i18n';

export type TicketRoutePlanHint = {
  status: 'pending' | 'blocked_oauth' | 'ready' | 'error';
  route?: 'native_endpoint' | 'local_bridge' | 'config_sync' | 'unsupported';
  canApply?: boolean;
  reason?: string;
};

export type TicketBindPurpose = 'share' | 'route';

export type TicketBindAction =
  | { disabled: false }
  | { disabled: true; reason: string };

/** @deprecated use TicketBindAction */
export type TicketRouteAction = TicketBindAction;

const ROUTE_DISABLED_FALLBACK = '这份登录目前不能走本机转发';
const ROUTE_NO_TARGET_FALLBACK = '没有可转发的目标工具';
const SHARE_DISABLED_FALLBACK = '这份登录目前不能直接用到其它工具';
const SHARE_NO_TARGET_FALLBACK = '没有可接到的目标工具';

/** Share = 直连 / 写进对方登录. Route = 本机转发 only. */
export function ticketRouteMatchesPurpose(
  route: TicketRoutePlanHint['route'],
  purpose: TicketBindPurpose,
): boolean {
  return bindRouteMatchesPurpose(route, purpose);
}

/** Disable a bind button only after every target settled with no applyable matching route. */
export function resolveTicketBindAction(
  hints: readonly TicketRoutePlanHint[],
  purpose: TicketBindPurpose,
  t?: TranslateFn,
): TicketBindAction {
  const noTarget = purpose === 'route'
    ? (t ? t('connections.list.routeNoTarget') : ROUTE_NO_TARGET_FALLBACK)
    : (t ? t('connections.list.shareNoTarget') : SHARE_NO_TARGET_FALLBACK);
  const generic = purpose === 'route'
    ? (t ? t('connections.list.routeDisabled') : ROUTE_DISABLED_FALLBACK)
    : (t ? t('connections.list.shareDisabled') : SHARE_DISABLED_FALLBACK);

  if (hints.length === 0) return { disabled: true, reason: noTarget };

  let pending = false;
  let sawConclusive = false;
  let matchingBlockedReason: string | undefined;
  let oauthReason: string | undefined;

  for (const hint of hints) {
    if (hint.status === 'pending') {
      pending = true;
      continue;
    }
    if (hint.status === 'ready' && ticketRouteMatchesPurpose(hint.route, purpose) && hint.canApply) {
      return { disabled: false };
    }
    if (hint.status === 'ready' && ticketRouteMatchesPurpose(hint.route, purpose) && !hint.canApply) {
      sawConclusive = true;
      matchingBlockedReason ??= hint.reason;
      continue;
    }
    if (hint.status === 'blocked_oauth') {
      sawConclusive = true;
      oauthReason ??= hint.reason;
      continue;
    }
    if (hint.status === 'ready') {
      sawConclusive = true;
    }
  }

  if (pending || !sawConclusive) return { disabled: false };
  if (matchingBlockedReason) return { disabled: true, reason: matchingBlockedReason };
  if (oauthReason) return { disabled: true, reason: oauthReason };
  return { disabled: true, reason: generic };
}

export function resolveTicketRouteAction(
  hints: readonly TicketRoutePlanHint[],
  t?: TranslateFn,
): TicketBindAction {
  return resolveTicketBindAction(hints, 'route', t);
}

export function resolveTicketShareAction(
  hints: readonly TicketRoutePlanHint[],
  t?: TranslateFn,
): TicketBindAction {
  return resolveTicketBindAction(hints, 'share', t);
}
