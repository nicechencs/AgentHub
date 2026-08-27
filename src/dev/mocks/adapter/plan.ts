import {
  adapterCommandError,
  type AdapterRouteRequest,
} from '@/lib/backend/contracts/adapter';
import { lookupGoldenExpect } from './golden-lookup';
import { planFromExpect, unsupportedPlan } from './project';
import {
  identifyRequestSource,
  type MockAdapterApplyPlan,
} from './source-product';
import { sourceExists } from './source-ticket';
import type { MockAdapterSourceResolver } from './types';

export type { MockAdapterApplyPlan } from './source-product';
export {
  identifyAccountSource,
  identifyProviderSource,
  identifyRequestSource,
  sourceProductOfPlan,
} from './source-product';

export function buildPlan(
  resolver: MockAdapterSourceResolver,
  request: AdapterRouteRequest,
): MockAdapterApplyPlan {
  if (!sourceExists(resolver, request)) {
    throw adapterCommandError({
      code: 'not_found',
      message: `${request.sourceKind} not found: ${request.sourceId}`,
      retryable: false,
    });
  }
  const sourceProduct = identifyRequestSource(
    resolver,
    request.sourceKind,
    request.sourceId,
  )?.product ?? 'other';
  const hit = lookupGoldenExpect(resolver, request, { record: false });
  const plan = hit ? planFromExpect(hit.expect, request) : unsupportedPlan(request);
  return { ...plan, sourceProduct };
}
