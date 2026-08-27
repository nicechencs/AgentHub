/**
 * Join key from live/golden source features to a frozen contract row.
 * Product identity comes from the plan/analyze owner; this file only maps
 * that identity onto SourceTicketKey (plus kimi-non-membership).
 * Does not choose route, reason, ruleId, or canApply.
 */
import type { AdapterRouteRequest } from '@/lib/backend/contracts/adapter';
import type { Provider } from '@/lib/types';
import type { MockSourceId } from '../source-classify';
import { identifyAccountSource, identifyProviderSource } from './source-product';
import type { ClassifiableAccount, MockAdapterSourceResolver } from './types';

export type SourceTicketKey =
  | MockSourceId
  | 'kimi-non-membership'
  | 'other'
  | 'missing';

export function sourceExists(
  resolver: MockAdapterSourceResolver,
  request: AdapterRouteRequest,
): boolean {
  if (request.sourceKind === 'provider') return !!resolver.getProviderById(request.sourceId);
  return !!resolver.getAccountById(request.sourceId);
}

export function ticketKeyFromProvider(provider: Provider): Exclude<SourceTicketKey, 'missing'> {
  const { mockSource } = identifyProviderSource(provider);
  if (mockSource) return mockSource;
  if (provider.agentId === 'kimi') return 'kimi-non-membership';
  return 'other';
}

export function ticketKeyFromAccount(account: ClassifiableAccount): Exclude<SourceTicketKey, 'missing'> {
  const { mockSource } = identifyAccountSource(account);
  if (mockSource) return mockSource;
  if (account.kind === 'apikey' && account.agentId === 'kimi') return 'kimi-non-membership';
  return 'other';
}

export function ticketKeyForRequest(
  resolver: MockAdapterSourceResolver,
  request: AdapterRouteRequest,
): SourceTicketKey {
  if (request.sourceKind === 'provider') {
    const provider = resolver.getProviderById(request.sourceId);
    if (!provider) return 'missing';
    return ticketKeyFromProvider(provider);
  }
  const account = resolver.getAccountById(request.sourceId) as ClassifiableAccount | undefined;
  if (!account) return 'missing';
  return ticketKeyFromAccount(account);
}
