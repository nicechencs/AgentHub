import {
  adapterCommandError,
  type AdapterApplyPlan,
  type AdapterRouteRequest,
} from '@/lib/backend/contracts/adapter';
import { lookupGoldenExpect } from './golden-lookup';
import { planFromExpect, unsupportedPlan } from './project';
import { sourceExists } from './source-ticket';
import type { MockAdapterSourceResolver } from './types';

export function buildPlan(
  resolver: MockAdapterSourceResolver,
  request: AdapterRouteRequest,
): AdapterApplyPlan {
  if (!sourceExists(resolver, request)) {
    throw adapterCommandError({
      code: 'not_found',
      message: `${request.sourceKind} not found: ${request.sourceId}`,
      retryable: false,
    });
  }
  const hit = lookupGoldenExpect(resolver, request, { record: false });
  if (!hit) return unsupportedPlan(request);
  return planFromExpect(hit.expect, request);
}
