import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { beforeEach, describe, expect, it } from 'vitest';
import { getBackend } from '@/app/runtime';
import { resetMockAccounts } from './account';
import { resetMockAgentStatuses } from './agent';
import {
  CONNECT_FLOW_FIXTURE_IDS,
  seedConnectFlowAdapterFixtures,
} from './connect-flow-fixtures';
import { createBackend } from './create-backend';
import { createMockProviderPort, getMockProviderById, resetMockProviders } from './provider';

describe('seedConnectFlowAdapterFixtures', () => {
  beforeEach(() => {
    resetMockProviders();
    resetMockAccounts();
    resetMockAgentStatuses();
  });

  it('seeds Kimi membership and Anthropic sources by default', () => {
    const seeded = seedConnectFlowAdapterFixtures();
    expect(seeded.kimiMembership.id).toBe(CONNECT_FLOW_FIXTURE_IDS.kimiMembership);
    expect(seeded.kimiMembership.preset).toBe('kimi-code-membership');
    expect(seeded.anthropic?.id).toBe(CONNECT_FLOW_FIXTURE_IDS.anthropic);
    expect(getMockProviderById(CONNECT_FLOW_FIXTURE_IDS.kimiMembership)?.id).toBe(
      CONNECT_FLOW_FIXTURE_IDS.kimiMembership,
    );
  });

  it('can omit the Anthropic fixture', () => {
    const seeded = seedConnectFlowAdapterFixtures({ includeAnthropic: false });
    expect(seeded.anthropic).toBeUndefined();
    expect(getMockProviderById(CONNECT_FLOW_FIXTURE_IDS.anthropic)).toBeUndefined();
  });

  it('does not hand-write apply success fields', () => {
    const src = readFileSync(
      path.join(path.dirname(fileURLToPath(import.meta.url)), 'connect-flow-fixtures.ts'),
      'utf8',
    );
    expect(src).not.toMatch(/ruleId/);
    expect(src).not.toMatch(/32123/);
    expect(src).not.toMatch(/state:\s*['"]running['"]/);
    expect(src).not.toMatch(/\bclassify(Account|Provider)Source\b/);
  });

  it('observes Kimi → Claude reshape and Codex bridge after seedBindings', async () => {
    getBackend();
    seedConnectFlowAdapterFixtures({ seedBindings: true });
    const wallet = await getBackend().ticket.listWallet();
    const kimiBindings = wallet.bindings.filter(
      (binding) => binding.ticketId === `provider:${CONNECT_FLOW_FIXTURE_IDS.kimiMembership}`,
    );
    expect(kimiBindings.some((binding) => binding.agentId === 'claude' && binding.route === 'reshape'))
      .toBe(true);
    expect(kimiBindings.some((binding) => binding.agentId === 'codex' && binding.route === 'bridge'))
      .toBe(true);
    expect(JSON.stringify(wallet)).not.toContain('must-not-leak');
  });
});

describe('createBackend connect-flow seed policy', () => {
  beforeEach(() => {
    resetMockProviders();
    resetMockAccounts();
    resetMockAgentStatuses();
  });

  it('keeps an empty pool under vitest after createBackend / getBackend', async () => {
    createBackend();
    const afterFactory = (await createMockProviderPort().listProviders()).map((item) => item.id);
    expect(afterFactory).not.toContain(CONNECT_FLOW_FIXTURE_IDS.kimiMembership);
    expect(afterFactory).not.toContain(CONNECT_FLOW_FIXTURE_IDS.anthropic);

    const afterGet = (await getBackend().provider.listProviders()).map((item) => item.id);
    expect(afterGet).not.toContain(CONNECT_FLOW_FIXTURE_IDS.kimiMembership);
    expect(afterGet).not.toContain(CONNECT_FLOW_FIXTURE_IDS.anthropic);
  });

  it('plans apply-ready Kimi→Claude, Kimi→Codex, and Anthropic→Pi after explicit seed', async () => {
    getBackend();
    seedConnectFlowAdapterFixtures();
    const adapter = getBackend().adapter;

    const kimiClaude = await adapter.plan({
      sourceKind: 'provider',
      sourceId: CONNECT_FLOW_FIXTURE_IDS.kimiMembership,
      targetAgentId: 'claude',
    });
    const kimiCodex = await adapter.plan({
      sourceKind: 'provider',
      sourceId: CONNECT_FLOW_FIXTURE_IDS.kimiMembership,
      targetAgentId: 'codex',
    });
    const anthropicPi = await adapter.plan({
      sourceKind: 'provider',
      sourceId: CONNECT_FLOW_FIXTURE_IDS.anthropic,
      targetAgentId: 'pi',
    });

    expect(kimiClaude.canApply).toBe(true);
    expect(kimiCodex.canApply).toBe(true);
    expect(anthropicPi.canApply).toBe(true);
  });

  it('plans a redacted official Claude login as apply-ready and rejects the stale Kimi member', async () => {
    getBackend();
    seedConnectFlowAdapterFixtures({ includeOauthAccount: true });
    const adapter = getBackend().adapter;

    const officialPi = await adapter.plan({
      sourceKind: 'account',
      sourceId: CONNECT_FLOW_FIXTURE_IDS.claudeOauth,
      targetAgentId: 'pi',
    });
    const staleClaude = await adapter.plan({
      sourceKind: 'account',
      sourceId: CONNECT_FLOW_FIXTURE_IDS.kimiMembershipStale,
      targetAgentId: 'claude',
    });

    expect(officialPi.canApply).toBe(true);
    expect(officialPi.analysis.ruleId).toBe('claude-subscription-to-pi-v1');
    expect(staleClaude.canApply).toBe(false);
    expect(JSON.stringify({ officialPi, staleClaude })).not.toContain('must-not-leak');
  });
});
