import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
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
  goldenTargetsForTicket,
  lookupGoldenExpect,
  ticketKeyForRequest,
} from './golden-lookup';
import type { SourceTicketKey } from './source-ticket';

type ContractCase = (typeof contract.cases)[number];

const adapterDir = path.dirname(fileURLToPath(import.meta.url));

const resolver = {
  getAccountById: getMockAccountById,
  getProviderById: getMockProviderById,
};

function frozenAccountHasUsableSecret(source: ContractCase['source']): boolean {
  if (source.kind === 'provider') return true;
  const credentials = 'credentials' in source ? source.credentials : undefined;
  if ((source.accountKind ?? 'oauth') === 'apikey') {
    return !!credentials
      && typeof credentials === 'object'
      && typeof (credentials as { format?: unknown }).format === 'string'
      && (credentials as { format: string }).format.trim().toLowerCase() === 'api_key'
      && typeof (credentials as { api_key?: unknown }).api_key === 'string'
      && Boolean((credentials as { api_key: string }).api_key.trim());
  }
  if (!credentials || typeof credentials !== 'object') return false;
  const record = credentials as Record<string, unknown>;
  const tokens = record.tokens as Record<string, unknown> | undefined;
  const bodyTokens = (record.body as Record<string, unknown> | undefined)?.tokens as
    | Record<string, unknown>
    | undefined;
  return [record.access_token, tokens?.access_token, bodyTokens?.access_token]
    .some((token) => typeof token === 'string' && Boolean(token.trim()));
}

function contractAccount(id: string, source: ContractCase['source']): Account {
  const hasSecret = frozenAccountHasUsableSecret(source);
  return {
    id,
    agentId: source.agentId as AgentId,
    kind: (source.accountKind ?? 'oauth') as Account['kind'],
    label: id,
    isCurrent: false,
    tokenValid: hasSecret,
    authHealth: hasSecret ? 'renewable' : 'needs_login',
    credentialFormat: 'credentialFormat' in source ? source.credentialFormat : undefined,
    credentials: 'credentials' in source ? source.credentials : undefined,
    extra: 'extra' in source ? source.extra : undefined,
  } as Account & {
    credentials?: Record<string, unknown>;
    extra?: Record<string, unknown>;
  };
}

function upsertLiveAccount(
  account: Omit<Account, 'tokenValid'> & {
    tokenValid?: boolean;
    credentials?: Record<string, unknown>;
    extra?: Record<string, unknown>;
  },
): void {
  upsertMockAccount(account as Account);
}

async function planFor(
  sourceKind: 'account' | 'provider',
  sourceId: string,
  targetAgentId: AgentId,
) {
  const adapter = createMockAdapterPort(resolver);
  return adapter.plan({ sourceKind, sourceId, targetAgentId });
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

  it('does not keep a classifier fallback or second route selector', () => {
    // Proves the old classifier / rule-fixtures path did not return.
    // project.ts still keys demo materialization by ruleId; this is not a
    // claim that the full plan has zero TypeScript projection.
    const files = [
      'analyze.ts',
      'plan.ts',
      'apply.ts',
      'project.ts',
      'golden-lookup.ts',
      'adapter.ts',
    ].map((name) => {
      const file = name === 'adapter.ts'
        ? path.join(adapterDir, '..', 'adapter.ts')
        : path.join(adapterDir, name);
      return readFileSync(file, 'utf8');
    });
    const blob = files.join('\n');
    expect(blob).not.toMatch(/analyzeFromClassifier/);
    expect(blob).not.toMatch(/from '\.\/classify'/);
    expect(blob).not.toMatch(/from '\.\/rule-fixtures'/);
    expect(blob).not.toMatch(/Keep in lockstep with/);
    expect(blob).not.toMatch(/SAME_EDGE_UNWRITABLE/);
    expect(blob).not.toMatch(/unsupportedReasonFromGraph/);
    expect(blob).not.toMatch(/findRuleFixture/);
    expect(blob).not.toMatch(/MOCK_RULE_FIXTURES/);
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
    expect(stats.knownSeedHits).toBeGreaterThan(0);
    expect(stats.hits).toBe(stats.lookups);
  });

  it('hits golden for known dev:mock seeds on every covered target and never misses', async () => {
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
      const ticketKey = ticketKeyForRequest(resolver, {
        sourceKind: seed.sourceKind,
        sourceId: seed.sourceId,
        targetAgentId: 'claude',
      });
      expect(ticketKey).not.toBe('missing');
      const targets = goldenTargetsForTicket(
        seed.sourceKind,
        ticketKey as Exclude<SourceTicketKey, 'missing'>,
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
  });

  it('returns unsupported for identities absent from golden and counts the miss', async () => {
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

  it('hits golden for a redacted but valid official login', async () => {
    seedConnectFlowAdapterFixtures({ includeOauthAccount: true });
    const account = getMockAccountById(CONNECT_FLOW_FIXTURE_IDS.claudeOauth);
    expect(account).toMatchObject({
      tokenValid: true,
      authHealth: 'renewable',
    });
    expect(account).not.toHaveProperty('credentials');

    const hit = lookupGoldenExpect(resolver, {
      sourceKind: 'account',
      sourceId: CONNECT_FLOW_FIXTURE_IDS.claudeOauth,
      targetAgentId: 'pi',
    });
    expect(hit?.id).toBe('claude-oauth-to-pi');
    const plan = await planFor('account', CONNECT_FLOW_FIXTURE_IDS.claudeOauth, 'pi');
    expect(plan.canApply).toBe(true);
    expect(plan.analysis.ruleId).toBe('claude-subscription-to-pi-v1');
    expect(JSON.stringify(plan)).not.toContain('must-not-leak');
  });

  it('hits secret=true golden when credentials are empty but tokenValid is true', async () => {
    upsertLiveAccount({
      id: 'claude-empty-bag-valid',
      agentId: 'claude',
      kind: 'oauth',
      label: 'Claude empty bag valid',
      isCurrent: false,
      tokenValid: true,
      authHealth: 'renewable',
      credentials: {},
    });

    const hit = lookupGoldenExpect(resolver, {
      sourceKind: 'account',
      sourceId: 'claude-empty-bag-valid',
      targetAgentId: 'pi',
    });
    expect(hit?.id).toBe('claude-oauth-to-pi');
    const plan = await planFor('account', 'claude-empty-bag-valid', 'pi');
    expect(plan.canApply).toBe(true);
    expect(JSON.stringify(plan)).not.toContain('must-not-leak');
  });

  it('does not apply empty credentials when tokenValid is false', async () => {
    upsertLiveAccount({
      id: 'claude-empty-bag-invalid',
      agentId: 'claude',
      kind: 'oauth',
      label: 'Claude empty bag invalid',
      isCurrent: false,
      tokenValid: false,
      credentials: {},
    });

    const plan = await planFor('account', 'claude-empty-bag-invalid', 'pi');
    expect(plan.canApply).toBe(false);
    expect(JSON.stringify(plan)).not.toContain('must-not-leak');
  });

  it('fails closed when status is unknown and credentials are empty', async () => {
    upsertLiveAccount({
      id: 'kimi-membership-unknown-empty',
      agentId: 'kimi',
      kind: 'apikey',
      label: 'Kimi membership unknown empty',
      isCurrent: false,
      extra: { provider: 'kimi-code-membership' },
      credentials: {},
    });

    const request = {
      sourceKind: 'account' as const,
      sourceId: 'kimi-membership-unknown-empty',
      targetAgentId: 'claude' as AgentId,
    };
    expect(lookupGoldenExpect(resolver, request)).toBeNull();
    const plan = await planFor('account', 'kimi-membership-unknown-empty', 'claude');
    expect(plan.canApply).toBe(false);
    expect(plan.analysis.route).toBe('unsupported');
    expect(JSON.stringify(plan)).not.toContain('must-not-leak');
  });

  it('does not treat an empty token slot as a usable secret when status is unknown', async () => {
    upsertLiveAccount({
      id: 'claude-unknown-empty-slot',
      agentId: 'claude',
      kind: 'oauth',
      label: 'Claude unknown empty slot',
      isCurrent: false,
      credentials: { access_token: '' },
    });

    const plan = await planFor('account', 'claude-unknown-empty-slot', 'pi');
    expect(plan.canApply).toBe(false);
    expect(JSON.stringify(plan)).not.toContain('must-not-leak');
  });

  it('hits golden when status is unknown but credentials contain a usable token', async () => {
    upsertLiveAccount({
      id: 'kimi-membership-unknown-with-key',
      agentId: 'kimi',
      kind: 'apikey',
      label: 'Kimi membership unknown with key',
      isCurrent: false,
      extra: { provider: 'kimi-code-membership' },
      credentials: { format: 'api_key', api_key: 'must-not-leak' },
    });
    upsertLiveAccount({
      id: 'claude-unknown-with-token',
      agentId: 'claude',
      kind: 'oauth',
      label: 'Claude unknown with token',
      isCurrent: false,
      credentials: { access_token: 'must-not-leak' },
    });

    const kimiHit = lookupGoldenExpect(resolver, {
      sourceKind: 'account',
      sourceId: 'kimi-membership-unknown-with-key',
      targetAgentId: 'claude',
    });
    expect(kimiHit?.id).toBe('kimi-membership-account-to-claude');
    const kimiPlan = await planFor('account', 'kimi-membership-unknown-with-key', 'claude');
    expect(kimiPlan.canApply).toBe(true);

    const claudeHit = lookupGoldenExpect(resolver, {
      sourceKind: 'account',
      sourceId: 'claude-unknown-with-token',
      targetAgentId: 'pi',
    });
    expect(claudeHit?.id).toBe('claude-oauth-to-pi');
    const claudePlan = await planFor('account', 'claude-unknown-with-token', 'pi');
    expect(claudePlan.canApply).toBe(true);
    expect(JSON.stringify({ kimiPlan, claudePlan })).not.toContain('must-not-leak');
  });

  it('does not apply a Kimi membership account with tokenValid false / needs_login', async () => {
    seedConnectFlowAdapterFixtures();
    const stale = await planFor(
      'account',
      CONNECT_FLOW_FIXTURE_IDS.kimiMembershipStale,
      'claude',
    );
    expect(stale.canApply).toBe(false);

    upsertLiveAccount({
      id: 'kimi-membership-needs-login',
      agentId: 'kimi',
      kind: 'apikey',
      label: 'Kimi membership needs login',
      isCurrent: false,
      tokenValid: false,
      authHealth: 'needs_login',
      extra: { provider: 'kimi-code-membership' },
    });

    const membership = await planFor('account', 'kimi-membership-needs-login', 'claude');
    expect(membership.canApply).toBe(false);
    expect(JSON.stringify({ stale, membership })).not.toContain('must-not-leak');
  });

  it('does not let a unique secret:true candidate apply Claude/Codex/Grok without a usable token', async () => {
    upsertLiveAccount({
      id: 'claude-no-token',
      agentId: 'claude',
      kind: 'oauth',
      label: 'Claude no token',
      isCurrent: false,
      tokenValid: false,
      authHealth: 'needs_login',
    });
    upsertLiveAccount({
      id: 'codex-auth-json-no-token',
      agentId: 'codex',
      kind: 'oauth',
      label: 'Codex no token',
      isCurrent: false,
      tokenValid: false,
      authHealth: 'needs_login',
      credentialFormat: 'auth_json',
    });
    upsertLiveAccount({
      id: 'grok-no-token',
      agentId: 'grok',
      kind: 'oauth',
      label: 'Grok no token',
      isCurrent: false,
      tokenValid: false,
      authHealth: 'needs_login',
    });

    const claudePi = await planFor('account', 'claude-no-token', 'pi');
    const codexGrok = await planFor('account', 'codex-auth-json-no-token', 'grok');
    const grokPi = await planFor('account', 'grok-no-token', 'pi');

    expect(claudePi.canApply).toBe(false);
    expect(codexGrok.canApply).toBe(false);
    expect(grokPi.canApply).toBe(false);
    expect(JSON.stringify({ claudePi, codexGrok, grokPi })).not.toContain('must-not-leak');
  });

  it('returns unsupported when credential availability has no exact golden candidate', async () => {
    upsertLiveAccount({
      id: 'kimi-membership-no-key',
      agentId: 'kimi',
      kind: 'apikey',
      label: 'Kimi membership no key',
      isCurrent: false,
      tokenValid: false,
      authHealth: 'needs_login',
      extra: { provider: 'kimi-code-membership' },
    });

    const request = {
      sourceKind: 'account' as const,
      sourceId: 'kimi-membership-no-key',
      targetAgentId: 'claude' as AgentId,
    };
    expect(lookupGoldenExpect(resolver, request)).toBeNull();
    const plan = await planFor('account', 'kimi-membership-no-key', 'claude');
    expect(plan.canApply).toBe(false);
    expect(plan.analysis.route).toBe('unsupported');
  });
});
