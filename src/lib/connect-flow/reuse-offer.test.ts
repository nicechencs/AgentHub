import { describe, expect, it } from 'vitest';
import type { Account, Provider } from '@/lib/types';
import {
  connectionCanReuseToOtherAgents,
  isAnthropicApiProvider,
  isKimiMembershipProvider,
  shouldShowReuseAction,
} from './reuse-offer';

function provider(overrides: Partial<Provider> = {}): Provider {
  return {
    id: 'prov-1',
    agentId: 'kimi',
    name: 'Kimi',
    preset: 'custom',
    configText: '',
    configFormat: 'toml',
    isCurrent: false,
    ...overrides,
  };
}

function account(overrides: Partial<Account> = {}): Account {
  return {
    id: 'acc-1',
    agentId: 'claude',
    kind: 'oauth',
    label: 'me@example.com',
    isCurrent: false,
    tokenValid: true,
    ...overrides,
  };
}

describe('isKimiMembershipProvider', () => {
  it('accepts the membership preset', () => {
    expect(isKimiMembershipProvider(provider({ preset: 'kimi-code-membership' }))).toBe(true);
  });

  it('accepts the official coding endpoint without preset', () => {
    expect(isKimiMembershipProvider(provider({
      preset: 'custom',
      configText: 'base_url = "https://api.kimi.com/coding/v1"',
    }))).toBe(true);
  });

  it('does not upgrade moonshot / open-platform Kimi', () => {
    expect(isKimiMembershipProvider(provider({
      preset: 'moonshot',
      configText: 'base_url = "https://api.moonshot.cn/v1"',
    }))).toBe(false);
  });
});

describe('isAnthropicApiProvider', () => {
  it('accepts the anthropic preset on Claude', () => {
    expect(isAnthropicApiProvider(provider({
      agentId: 'claude',
      preset: 'anthropic',
    }))).toBe(true);
  });

  it('accepts api.anthropic.com in config', () => {
    expect(isAnthropicApiProvider(provider({
      agentId: 'claude',
      preset: 'custom',
      configText: '{"ANTHROPIC_BASE_URL":"https://api.anthropic.com"}',
    }))).toBe(true);
  });

  it('rejects Claude official-login shaped providers without Anthropic API', () => {
    expect(isAnthropicApiProvider(provider({
      agentId: 'claude',
      preset: 'custom',
      configText: '{}',
    }))).toBe(false);
  });
});

describe('connectionCanReuseToOtherAgents', () => {
  it('offers Kimi membership providers', () => {
    const kimi = provider({ id: 'kimi-1', preset: 'kimi-code-membership' });
    expect(connectionCanReuseToOtherAgents({
      source: 'provider',
      id: kimi.id,
      agentId: kimi.agentId,
      provider: kimi,
    })).toBe(true);
  });

  it('offers Claude Anthropic API providers', () => {
    const anthropic = provider({ id: 'ant-1', agentId: 'claude', preset: 'anthropic' });
    expect(connectionCanReuseToOtherAgents({
      source: 'provider',
      id: anthropic.id,
      agentId: anthropic.agentId,
      provider: anthropic,
    })).toBe(true);
  });

  it('hides every account source — implemented_apply_whitelist closes non-Provider', () => {
    // adapter_route_service.rs: if request.source_kind != Provider { return false; }
    const cases: Account[] = [
      account({ id: 'claude-oauth', kind: 'oauth', agentId: 'claude' }),
      account({ id: 'codex-oauth', kind: 'oauth', agentId: 'codex' }),
      account({ id: 'claude-apikey', kind: 'apikey', agentId: 'claude' }),
      account({ id: 'pi-anthropic', kind: 'apikey', agentId: 'pi', provider: 'anthropic' }),
    ];
    for (const row of cases) {
      expect(connectionCanReuseToOtherAgents({
        source: 'account',
        id: row.id,
        agentId: row.agentId,
      })).toBe(false);
    }
  });

  it('rejects Claude official-login shaped providers without Anthropic API', () => {
    const official = provider({
      id: 'claude-official',
      agentId: 'claude',
      preset: 'custom',
      configText: '{}',
    });
    expect(connectionCanReuseToOtherAgents({
      source: 'provider',
      id: official.id,
      agentId: official.agentId,
      provider: official,
    })).toBe(false);
  });

  it('rejects provider source without a provider field', () => {
    expect(connectionCanReuseToOtherAgents({
      source: 'provider',
      id: 'missing-provider',
      agentId: 'claude',
    })).toBe(false);
  });

  it('hides moonshot Kimi providers', () => {
    const moonshot = provider({ id: 'kimi-open', preset: 'moonshot' });
    expect(connectionCanReuseToOtherAgents({
      source: 'provider',
      id: moonshot.id,
      agentId: moonshot.agentId,
      provider: moonshot,
    })).toBe(false);
  });
});

describe('shouldShowReuseAction', () => {
  const kimi = provider({ id: 'kimi-1', preset: 'kimi-code-membership' });
  const kimiEntry = {
    source: 'provider' as const,
    id: kimi.id,
    agentId: kimi.agentId,
    provider: kimi,
  };

  it('hides when the page has not wired reuse', () => {
    expect(shouldShowReuseAction(kimiEntry, { reuseEnabled: false })).toBe(false);
  });

  it('hides adapter-generated providers even if they look reusable', () => {
    expect(shouldShowReuseAction(kimiEntry, {
      reuseEnabled: true,
      adapterGeneratedProviderIds: new Set(['kimi-1']),
    })).toBe(false);
  });

  it('shows Kimi membership when reuse is wired', () => {
    expect(shouldShowReuseAction(kimiEntry, { reuseEnabled: true })).toBe(true);
  });

  it('hides account rows even when reuse is wired', () => {
    const oauth = account({ id: 'claude-oauth', kind: 'oauth' });
    expect(shouldShowReuseAction({
      source: 'account',
      id: oauth.id,
      agentId: oauth.agentId,
    }, { reuseEnabled: true })).toBe(false);
  });

  it('shows Claude Anthropic providers when reuse is wired', () => {
    const anthropic = provider({ id: 'ant-1', agentId: 'claude', preset: 'anthropic' });
    expect(shouldShowReuseAction({
      source: 'provider',
      id: anthropic.id,
      agentId: anthropic.agentId,
      provider: anthropic,
    }, { reuseEnabled: true })).toBe(true);
  });
});
