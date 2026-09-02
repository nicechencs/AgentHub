import { describe, expect, it } from 'vitest';
import type { Account, AgentKey, Provider } from '@/lib/types';
import {
  classifyAccountSource,
  classifyProviderSource,
  isKimiMembershipAccount,
  type ClassifiableAccount,
  type MockSourceId,
} from '@/dev/mocks/source-classify';
import {
  SOURCE_CLASSIFY_CONTRACT,
  productFromMockSource,
} from './source-classify-contract';

const MOCK_SOURCE_IDS: MockSourceId[] = [
  'kimi-code-membership',
  'anthropic',
  'openai',
  'xai',
  'glm-coding-plan',
  'deepseek-api',
  'claude-oauth',
  'grok-oauth',
  'codex-auth-json',
  'codex-oauth',
];

function accountFromCase(entry: (typeof SOURCE_CLASSIFY_CONTRACT.cases)[number]): ClassifiableAccount {
  return {
    id: entry.id,
    agentId: entry.agentId as AgentKey,
    kind: (entry.accountKind ?? 'apikey') as Account['kind'],
    label: entry.id,
    isCurrent: false,
    tokenValid: true,
    extra: 'extra' in entry ? entry.extra : undefined,
    credentials: 'credentials' in entry ? entry.credentials : undefined,
  };
}

function providerFromCase(entry: (typeof SOURCE_CLASSIFY_CONTRACT.cases)[number]): Provider {
  return {
    id: entry.id,
    agentId: entry.agentId as AgentKey,
    name: entry.id,
    preset: 'preset' in entry && typeof entry.preset === 'string' ? entry.preset : 'custom',
    configText:
      'configText' in entry && typeof entry.configText === 'string'
        ? entry.configText
        : JSON.stringify('settings' in entry ? entry.settings : {}),
    configFormat: 'json',
    isCurrent: false,
  };
}

describe('source-classify contract', () => {
  it('covers every mock source id and every product except other has a mock source', () => {
    const mapped = Object.keys(SOURCE_CLASSIFY_CONTRACT.mockSourceToProduct).sort();
    expect(mapped).toEqual([...MOCK_SOURCE_IDS].sort());
    const products = new Set(Object.values(SOURCE_CLASSIFY_CONTRACT.mockSourceToProduct));
    for (const product of SOURCE_CLASSIFY_CONTRACT.products) {
      if (product === 'other') continue;
      expect(products.has(product), `missing mock source for product ${product}`).toBe(true);
    }
  });

  it('classifies each case lockstep with the shared product table', () => {
    const classified = new Set<string>();
    for (const entry of SOURCE_CLASSIFY_CONTRACT.cases) {
      const got =
        entry.kind === 'account'
          ? productFromMockSource(
              (() => {
                const account = accountFromCase(entry);
                if (isKimiMembershipAccount(account)) return 'kimi-code-membership';
                return classifyAccountSource(account, {
                  includeGlmResponses: true,
                  includeAnthropicEndpoint: true,
                });
              })(),
            )
          : productFromMockSource(
              classifyProviderSource(providerFromCase(entry), { includeGlmResponses: true }),
            );
      expect(got, entry.id).toBe(entry.expectProduct);
      classified.add(entry.expectProduct);
    }
    for (const product of SOURCE_CLASSIFY_CONTRACT.products) {
      expect(classified.has(product), `no classify case for product ${product}`).toBe(true);
    }
  });
});
