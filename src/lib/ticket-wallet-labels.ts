/**
 * Ticket / binding route labels shared across Connections and Dashboard.
 */
import type { BindingRoute } from '@/lib/backend/contracts/ticket';
import { bindingRouteDashboardLabel } from '@/lib/backend/contracts/ticket';
import type { TranslateFn } from '@/lib/i18n';

/** Same connection-state words on Connections cards and Dashboard agent cards. */
export function connectionStateRouteLabel(route: BindingRoute, t?: TranslateFn): string {
  if (!t) return bindingRouteDashboardLabel(route);
  if (route === 'reshape') return t('connections.list.routeReshape');
  if (route === 'bridge') return t('kind.route.localRoute');
  return t('kind.route.direct');
}
