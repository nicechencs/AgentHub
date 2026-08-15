import type { Provider } from '@/lib/types';
import { markMockAgentInstalled } from './agent';
import { upsertMockProvider } from './provider';

/**
 * ConnectFlow / Adapter fixtures (Kimi membership + Anthropic API).
 *
 * Interactive `pnpm dev:mock` seeds these from `createBackend()` (default
 * includeAnthropic + markPiInstalled). Vitest `createBackend()` stays an empty
 * pool — call this after the factory or `getBackend()` to exercise apply.
 */
export const CONNECT_FLOW_FIXTURE_IDS = {
  kimiMembership: 'kimi-code-membership',
  anthropic: 'anthropic-api',
} as const;

const MUST_NOT_LEAK = 'must-not-leak';

export function connectFlowKimiMembershipProvider(): Provider {
  return {
    id: CONNECT_FLOW_FIXTURE_IDS.kimiMembership,
    agentId: 'kimi',
    name: 'Kimi Code 会员',
    preset: 'kimi-code-membership',
    configText: `api_key = "${MUST_NOT_LEAK}"`,
    configFormat: 'toml',
    isCurrent: false,
  };
}

export function connectFlowAnthropicProvider(): Provider {
  return {
    id: CONNECT_FLOW_FIXTURE_IDS.anthropic,
    agentId: 'claude',
    name: 'Anthropic API',
    preset: 'anthropic',
    configText: JSON.stringify({
      env: { ANTHROPIC_API_KEY: MUST_NOT_LEAK },
    }),
    configFormat: 'json',
    isCurrent: false,
  };
}

export function seedConnectFlowAdapterFixtures(options?: {
  includeAnthropic?: boolean;
  markPiInstalled?: boolean;
}): {
  kimiMembership: Provider;
  anthropic?: Provider;
} {
  const kimiMembership = upsertMockProvider(connectFlowKimiMembershipProvider());
  const anthropic = options?.includeAnthropic === false
    ? undefined
    : upsertMockProvider(connectFlowAnthropicProvider());
  if (options?.markPiInstalled !== false) {
    markMockAgentInstalled('pi');
  }
  return { kimiMembership, anthropic };
}
