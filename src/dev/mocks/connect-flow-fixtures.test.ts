import { beforeEach, describe, expect, it } from 'vitest';
import { getBackend } from '@/app/runtime';
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
});

describe('createBackend connect-flow seed policy', () => {
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
});
