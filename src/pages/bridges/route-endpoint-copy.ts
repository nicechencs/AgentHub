import type { MessageKey } from '@/lib/i18n';
import type { RouteEndpointId } from '@/lib/route-endpoints';
import { surfaceForCreateRouteTarget, type CreateRouteTarget } from './create-route-flow';

export const ROUTE_MODELS_PATH = '/models';

export type RouteLocalAddressRow = {
  method: 'GET' | 'POST';
  path: string;
  copyKey: MessageKey;
};

export const ROUTE_LOCAL_ADDRESS_LEGEND: readonly RouteLocalAddressRow[] = [
  { method: 'POST', path: '/v1/messages', copyKey: 'routes.endpoint.messages' },
  { method: 'POST', path: '/v1/responses', copyKey: 'routes.endpoint.responses' },
  { method: 'POST', path: '/v1/chat/completions', copyKey: 'routes.endpoint.chatCompletions' },
  { method: 'GET', path: ROUTE_MODELS_PATH, copyKey: 'routes.endpoint.models' },
];

export function routeEndpointCopyKey(id: RouteEndpointId): MessageKey {
  if (id === 'messages') return 'routes.endpoint.messages';
  if (id === 'responses') return 'routes.endpoint.responses';
  return 'routes.endpoint.chatCompletions';
}

export function localAddressCopyForTarget(target: CreateRouteTarget): {
  path: string;
  copyKey: MessageKey;
} {
  const surface = surfaceForCreateRouteTarget(target);
  return { path: surface.path, copyKey: routeEndpointCopyKey(surface.endpointId) };
}

export function formatInboundAt(at: string): string {
  if (at.length >= 19) return at.slice(0, 19).replace('T', ' ');
  return at;
}
