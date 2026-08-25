import {
  adapterCommandError,
  type AdapterRouteAnalysis,
  type AdapterRouteRequest,
} from '@/lib/backend/contracts/adapter';
import { lookupGoldenExpect } from './golden-lookup';
import { analysisFromExpect, unsupportedAnalysis } from './project';
import { sourceExists } from './source-ticket';
import type { MockAdapterSourceResolver } from './types';

export function analyze(
  resolver: MockAdapterSourceResolver,
  request: AdapterRouteRequest,
): AdapterRouteAnalysis {
  if (!sourceExists(resolver, request)) {
    throw adapterCommandError({
      code: 'not_found',
      message: `${request.sourceKind} not found: ${request.sourceId}`,
      retryable: false,
    });
  }
  const hit = lookupGoldenExpect(resolver, request);
  if (!hit) return unsupportedAnalysis();
  return analysisFromExpect(hit.expect, request);
}
