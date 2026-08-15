import type { Account, Provider } from '@/lib/types';
import type { AdapterBridgeRuntimeStatus, AdapterProfile } from '@/lib/backend/contracts';
import { markMockAgentInstalled } from './agent';
import { upsertMockAccount } from './account';
import { upsertMockProvider } from './provider';
import { seedMockAdapterProfiles } from './adapter';

/**
 * ConnectFlow / Adapter / ticket-wallet fixtures.
 *
 * Interactive `pnpm dev:mock` seeds these from `createBackend()`. Vitest
 * `createBackend()` stays an empty pool — call this after the factory.
 *
 * Seeded tickets (at least):
 * - Kimi membership (bound Claude reshape + Codex bridge when profiles seeded)
 * - Anthropic API
 * - unknown custom provider
 * - official-login account (Claude OAuth)
 */
export const CONNECT_FLOW_FIXTURE_IDS = {
  kimiMembership: 'kimi-code-membership',
  anthropic: 'anthropic-api',
  unknownProvider: 'unknown-custom-relay',
  claudeOauth: 'claude-official-login',
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
    isCurrent: true,
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

export function connectFlowUnknownProvider(): Provider {
  return {
    id: CONNECT_FLOW_FIXTURE_IDS.unknownProvider,
    agentId: 'claude',
    name: '自定义中转',
    preset: 'custom',
    configText: JSON.stringify({
      env: { ANTHROPIC_BASE_URL: 'https://relay.example.invalid/v1' },
    }),
    configFormat: 'json',
    isCurrent: false,
  };
}

export function connectFlowClaudeOauthAccount(): Account {
  return {
    id: CONNECT_FLOW_FIXTURE_IDS.claudeOauth,
    agentId: 'claude',
    kind: 'oauth',
    label: 'me@example.com',
    email: 'me@example.com',
    isCurrent: false,
    tokenValid: true,
    authHealth: 'renewable',
  };
}

/**
 * Seed demo profiles so wallet bindings show Kimi → Claude reshape + Codex bridge.
 * Requires at least one live mock adapter port (createBackend already created).
 */
export function seedTicketWalletBindingProfiles(): {
  claudeProfile: AdapterProfile;
  codexProfile: AdapterProfile;
} {
  const now = '2026-08-15T00:00:00.000Z';
  const kimiId = CONNECT_FLOW_FIXTURE_IDS.kimiMembership;
  const claudeGenId = `claude-kimi-adapter-${kimiId}`;
  const codexGenId = `codex-kimi-bridge-${kimiId}`;

  upsertMockProvider({
    id: claudeGenId,
    agentId: 'claude',
    name: 'Kimi → Claude (demo)',
    preset: 'anthropic-compatible',
    configText: JSON.stringify({
      env: {
        ANTHROPIC_BASE_URL: 'https://api.kimi.com/coding/',
        ANTHROPIC_AUTH_TOKEN: '$AGENTHUB_CONNECTION_SECRET$',
      },
    }),
    configFormat: 'json',
    isCurrent: true,
  });
  upsertMockProvider({
    id: codexGenId,
    agentId: 'codex',
    name: 'Kimi → Codex 本地桥接 (demo)',
    preset: 'openai-compatible',
    configText: JSON.stringify({
      baseUrl: 'http://127.0.0.1:32123/v1',
      model: 'kimi-k2.5',
    }),
    configFormat: 'json',
    isCurrent: true,
  });

  const claudeProfile: AdapterProfile = {
    id: `adapter-kimi-claude-${kimiId}`,
    name: `Kimi → Claude (${kimiId})`,
    sourceKind: 'provider',
    sourceId: kimiId,
    targetAgentId: 'claude',
    route: 'native_endpoint',
    mode: 'api',
    status: 'active',
    ruleId: 'kimi-membership-to-claude-v1',
    ruleVersion: '1',
    generatedProviderId: claudeGenId,
    localPort: null,
    autoStart: false,
    createdAt: now,
    updatedAt: now,
  };
  const codexProfile: AdapterProfile = {
    id: `adapter-kimi-codex-bridge-${kimiId}`,
    name: `Kimi → Codex 本地桥接 (${kimiId})`,
    sourceKind: 'provider',
    sourceId: kimiId,
    targetAgentId: 'codex',
    route: 'local_bridge',
    mode: 'api',
    status: 'active',
    ruleId: 'kimi-membership-to-codex-v1',
    ruleVersion: '1',
    generatedProviderId: codexGenId,
    localPort: 32123,
    autoStart: false,
    createdAt: now,
    updatedAt: now,
  };

  const bridge: AdapterBridgeRuntimeStatus = {
    profileId: codexProfile.id,
    state: 'running',
    port: 32123,
    endpoint: 'http://127.0.0.1:32123/v1',
    startedAt: now,
    upstreamStatus: 'unknown',
  };
  seedMockAdapterProfiles([claudeProfile, codexProfile], { [codexProfile.id]: bridge });

  return { claudeProfile, codexProfile };
}

export function seedConnectFlowAdapterFixtures(options?: {
  includeAnthropic?: boolean;
  includeUnknown?: boolean;
  includeOauthAccount?: boolean;
  markPiInstalled?: boolean;
  seedBindings?: boolean;
}): {
  kimiMembership: Provider;
  anthropic?: Provider;
  unknown?: Provider;
  oauthAccount?: Account;
} {
  const kimiMembership = upsertMockProvider(connectFlowKimiMembershipProvider());
  const anthropic = options?.includeAnthropic === false
    ? undefined
    : upsertMockProvider(connectFlowAnthropicProvider());
  const unknown = options?.includeUnknown === true
    ? upsertMockProvider(connectFlowUnknownProvider())
    : undefined;
  const oauthAccount = options?.includeOauthAccount === true
    ? upsertMockAccount(connectFlowClaudeOauthAccount())
    : undefined;
  if (options?.markPiInstalled !== false) {
    markMockAgentInstalled('pi');
  }
  if (options?.seedBindings === true) {
    seedTicketWalletBindingProfiles();
  }
  return { kimiMembership, anthropic, unknown, oauthAccount };
}
