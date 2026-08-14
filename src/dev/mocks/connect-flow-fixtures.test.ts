import { beforeEach, describe, expect, it } from 'vitest';
import { getMockProviderById, resetMockProviders } from './provider';
import {
  CONNECT_FLOW_FIXTURE_IDS,
  seedConnectFlowAdapterFixtures,
} from './connect-flow-fixtures';
import { resetMockAgentStatuses } from './agent';

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
