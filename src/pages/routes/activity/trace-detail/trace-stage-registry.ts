import type { MessageKey } from '@/lib/i18n';
import type { RouteTraceStageId } from '@/lib/backend/contracts/adapter';

export const TRACE_STAGE_REGISTRY: ReadonlyArray<{
  id: RouteTraceStageId;
  titleKey: MessageKey;
}> = [
  { id: 'received', titleKey: 'routes.trace.detailStage.received' },
  { id: 'local_auth', titleKey: 'routes.trace.detailStage.localAuth' },
  { id: 'local_endpoint', titleKey: 'routes.trace.detailStage.localEndpoint' },
  { id: 'admission', titleKey: 'routes.trace.detailStage.admission' },
  { id: 'route_resolution', titleKey: 'routes.trace.detailStage.routeResolution' },
  { id: 'pool', titleKey: 'routes.trace.detailStage.pool' },
  { id: 'request_conversion', titleKey: 'routes.trace.detailStage.requestConversion' },
  { id: 'upstream_request', titleKey: 'routes.trace.detailStage.upstreamRequest' },
  { id: 'upstream_response', titleKey: 'routes.trace.detailStage.upstreamResponse' },
  { id: 'response_conversion', titleKey: 'routes.trace.detailStage.responseConversion' },
  { id: 'delivery', titleKey: 'routes.trace.detailStage.delivery' },
];
