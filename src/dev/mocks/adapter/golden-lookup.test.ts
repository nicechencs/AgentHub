import { beforeEach, describe, expect, it } from 'vitest';
import type { Account, AgentId, Provider } from '@/lib/types';
import {
  CONNECT_FLOW_FIXTURE_IDS,
  seedConnectFlowAdapterFixtures,
} from '../connect-flow-fixtures';
import contract from '../fixtures/adapter-capability-contract.json';
import {
  createMockAdapterPort,
  DEV_MOCK_KNOWN_SEED_IDS,
  getGoldenLookupStats,
  resetGoldenLookupStats,
  resetMockAdapters,
} from '../adapter';
import { getMockAccountById, resetMockAccounts, upsertMockAccount } from '../account';
import {
  createMockProviderPort,
  getMockProviderById,
  resetMockProviders,
  upsertMockProvider,
} from '../provider';
import {
  classifyLiveSource,
  goldenTargetsForIdentity,
  lookupGoldenExpect,
} from './golden-lookup';
import type { RouteSourceLabel } from './types';

type ContractCase = (typeof contract.cases)[number];

const resolver = {
  getAccountById: getMockAccountById,
  getProviderById: getMockProviderById,
};

function contractAccount(id: string, source: ContractCase['source']): Account {
  return {
    id,
    agentId: source.agentId as AgentId,
    kind: (source.accountKind ?? 'oauth') as Account['kind'],
    label: id,
    isCurrent: false,
    tokenValid: true,
    credentialFormat: 'credentialFormat' in source ? source.credentialFormat : undefined,
    credentials: 'credentials' in source ? source.credentials : undefined,
    extra: 'extra' in source ? source.extra : undefined,
  } as Account & {
    credentials?: Record<string, unknown>;
    extra?: Record<string, unknown>;
  };
}

function seedContractCase(item: ContractCase): string {
  const sourceId = `contract-${item.id}`;
  if (item.source.kind === 'provider') {
    upsertMockProvider({
      id: sourceId,
      agentId: item.source.agentId as AgentId,
      name: item.id,
      preset: item.source.preset ?? 'default',
      configText: '{}',
      configFormat: 'json',
      isCurrent: false,
    } satisfies Provider);
  } else {
    upsertMockAccount(contractAccount(sourceId, item.source));
  }
  return sourceId;
}

describe('mock golden lookup', () => {
  beforeEach(() => {
    resetMockAdapters();
    resetMockProviders();
    resetMockAccounts();
    resetGoldenLookupStats();
  });

  it('keeps connect-flow seed ids aligned with the known-seed list', () => {
    expect([...DEV_MOCK_KNOWN_SEED_IDS]).toEqual([
      CONNECT_FLOW_FIXTURE_IDS.kimiMembership,
      CONNECT_FLOW_FIXTURE_IDS.anthropic,
      CONNECT_FLOW_FIXTURE_IDS.claudeOauth,
    ]);
  });

  it('hits golden.expect for every shared contract case and reports zero known-seed misses', async () => {
    const adapter = createMockAdapterPort(resolver);
    const leaked: string[] = [];
    for (const item of contract.cases) {
      const sourceId = seedContractCase(item);
      const request = {
        sourceKind: item.source.kind as 'account' | 'provider',
        sourceId,
        targetAgentId: item.target as AgentId,
      };
      const hit = lookupGoldenExpect(resolver, request);
      expect(hit, `${item.id} must hit golden`).not.toBeNull();
      expect(hit?.expect.route).toBe(item.expect.route);
      expect(hit?.expect.canApply).toBe(item.expect.canApply);
      expect(hit?.expect.ruleId ?? null).toBe(item.expect.ruleId);
      const plan = await adapter.plan(request);
      expect(plan.analysis.route).toBe(item.expect.route);
      expect(plan.canApply).toBe(item.expect.canApply);
      expect(plan.reason).toBe(item.expect.reason);
      const blob = JSON.stringify(plan);
      expect(blob).not.toContain('must-not-leak');
      if ('credentials' in item.source && item.source.credentials) {
        const secrets = JSON.stringify(item.source.credentials);
        if (secrets.includes('must-not-leak') && blob.includes('must-not-leak')) {
          leaked.push(item.id);
        }
      }
    }
    expect(leaked).toEqual([]);
    const stats = getGoldenLookupStats();
    expect(stats.knownSeedMisses).toBe(0);
    expect(stats.misses).toBe(0);
    expect(stats.fallbacks).toBe(0);
    expect(stats.knownSeedHits).toBeGreaterThan(0);
    expect(stats.hits).toBe(stats.lookups);
  });

  it('hits golden for known dev:mock seeds on every covered target', async () => {
    seedConnectFlowAdapterFixtures({
      includeUnknown: true,
      includeOauthAccount: true,
    });
    const adapter = createMockAdapterPort(resolver);
    const seeds: Array<{
      sourceKind: 'account' | 'provider';
      sourceId: string;
    }> = [
      { sourceKind: 'provider', sourceId: CONNECT_FLOW_FIXTURE_IDS.kimiMembership },
      { sourceKind: 'provider', sourceId: CONNECT_FLOW_FIXTURE_IDS.anthropic },
      { sourceKind: 'account', sourceId: CONNECT_FLOW_FIXTURE_IDS.claudeOauth },
    ];

    for (const seed of seeds) {
      const classified = classifyLiveSource(resolver, {
        sourceKind: seed.sourceKind,
        sourceId: seed.sourceId,
        targetAgentId: 'claude',
      });
      expect(classified).not.toBe('not_found');
      const targets = goldenTargetsForIdentity(
        seed.sourceKind,
        classified as Exclude<RouteSourceLabel, 'not_found'>,
      );
      expect(targets.length, `${seed.sourceId} must have golden targets`).toBeGreaterThan(0);
      for (const target of targets) {
        const request = {
          sourceKind: seed.sourceKind,
          sourceId: seed.sourceId,
          targetAgentId: target as AgentId,
        };
        const hit = lookupGoldenExpect(resolver, request);
        expect(hit, `${seed.sourceId} → ${target}`).not.toBeNull();
        const plan = await adapter.plan(request);
        expect(plan.analysis.route).toBe(hit!.expect.route);
        expect(plan.canApply).toBe(hit!.expect.canApply);
        expect(JSON.stringify(plan)).not.toContain('must-not-leak');
      }
    }

    const stats = getGoldenLookupStats();
    expect(stats.knownSeedMisses).toBe(0);
    expect(stats.misses).toBe(0);
    expect(stats.fallbacks).toBe(0);
  });

  it('falls back to the old classifier for identities absent from golden and counts the miss', async () => {
    const sourceId = 'dsh-only-no-golden';
    await createMockProviderPort().upsertProvider({
      id: sourceId,
      agentId: 'dsh',
      name: 'DSH row',
      preset: 'default',
      configText: '{"apiKey":"must-not-leak"}',
      configFormat: 'json',
      isCurrent: false,
    });
    const adapter = createMockAdapterPort(resolver);
    const request = {
      sourceKind: 'provider' as const,
      sourceId,
      targetAgentId: 'dsh' as AgentId,
    };
    expect(lookupGoldenExpect(resolver, request)).toBeNull();
    const plan = await adapter.plan(request);
    expect(plan.canApply).toBe(false);
    expect(plan.analysis.route).toBe('unsupported');
    expect(JSON.stringify(plan)).not.toContain('must-not-leak');
    const stats = getGoldenLookupStats();
    expect(stats.misses).toBeGreaterThan(0);
    expect(stats.fallbacks).toBe(stats.misses);
    expect(stats.knownSeedMisses).toBe(0);
  });

  it('keeps unknown custom sources fail-closed without leaking the placeholder', async () => {
    const { unknown } = seedConnectFlowAdapterFixtures({ includeUnknown: true });
    expect(unknown).toBeDefined();
    const adapter = createMockAdapterPort(resolver);
    const plan = await adapter.plan({
      sourceKind: 'provider',
      sourceId: unknown!.id,
      targetAgentId: 'claude',
    });
    expect(plan.canApply).toBe(false);
    expect(plan.analysis.route).toBe('unsupported');
    expect(JSON.stringify(plan)).not.toContain('must-not-leak');
  });
});
