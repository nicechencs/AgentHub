/**
 * Plan/analyze-owned source identity. Runtime mock modules (ticket wallet,
 * source-ticket join keys) read product from here or from plan(), not by
 * calling classify* themselves.
 *
 * classifyAccountSource / classifyProviderSource stay in source-classify.ts
 * for this owner and for source-classify-contract.test.ts.
 */
import type { AdapterApplyPlan } from '@/lib/backend/contracts/adapter';
import {
  productFromMockSource,
  type ClassifyProductId,
} from '@/lib/backend/contracts/source-classify-contract';
import type { Provider } from '@/lib/types';
import {
  classifyAccountSource,
  classifyProviderSource,
  isKimiMembershipAccount,
  type ClassifiableAccount,
  type MockSourceId,
} from '../source-classify';
import type { MockAdapterSourceResolver } from './types';

const PLAN_ACCOUNT_CLASSIFY = {
  includeGlmResponses: true,
  includeAnthropicEndpoint: true,
} as const;

const PLAN_PROVIDER_CLASSIFY = {
  includeGlmResponses: true,
} as const;

export type MockSourceIdentity = {
  mockSource: MockSourceId | null;
  product: ClassifyProductId;
};

export type MockAdapterApplyPlan = AdapterApplyPlan & {
  sourceProduct: ClassifyProductId;
};

export function identifyAccountSource(account: ClassifiableAccount): MockSourceIdentity {
  const mockSource = isKimiMembershipAccount(account)
    ? 'kimi-code-membership'
    : classifyAccountSource(account, PLAN_ACCOUNT_CLASSIFY);
  return { mockSource, product: productFromMockSource(mockSource) };
}

export function identifyProviderSource(provider: Provider): MockSourceIdentity {
  const mockSource = classifyProviderSource(provider, PLAN_PROVIDER_CLASSIFY);
  return { mockSource, product: productFromMockSource(mockSource) };
}

export function identifyRequestSource(
  resolver: MockAdapterSourceResolver,
  sourceKind: 'account' | 'provider',
  sourceId: string,
): MockSourceIdentity | null {
  if (sourceKind === 'provider') {
    const provider = resolver.getProviderById(sourceId);
    if (!provider) return null;
    return identifyProviderSource(provider);
  }
  const account = resolver.getAccountById(sourceId) as ClassifiableAccount | undefined;
  if (!account) return null;
  return identifyAccountSource(account);
}

export function sourceProductOfPlan(plan: AdapterApplyPlan): ClassifyProductId {
  const raw = (plan as Partial<MockAdapterApplyPlan>).sourceProduct;
  if (typeof raw === 'string') return raw;
  return 'other';
}
