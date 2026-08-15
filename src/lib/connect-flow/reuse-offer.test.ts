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

describe('isKimiMembershipProvider (deprecated surface helper, not a gate)', () => {
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

describe('isAnthropicApiProvider (deprecated surface helper, not a gate)', () => {
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

describe('connectionCanReuseToOtherAgents (true tickets always)', () => {
  it('offers every account ticket including OAuth and API Key', () => {
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
      })).toBe(true);
    }
  });

  it('offers Kimi membership, Anthropic, moonshot, and unknown providers alike', () => {
    const rows = [
      provider({ id: 'kimi-1', preset: 'kimi-code-membership' }),
      provider({ id: 'ant-1', agentId: 'claude', preset: 'anthropic' }),
      provider({ id: 'kimi-open', preset: 'moonshot' }),
      provider({
        id: 'claude-official',
        agentId: 'claude',
        preset: 'custom',
        configText: '{}',
      }),
    ];
    for (const row of rows) {
      expect(connectionCanReuseToOtherAgents({
        source: 'provider',
        id: row.id,
        agentId: row.agentId,
        provider: row,
      })).toBe(true);
    }
  });

  it('rejects provider source without an id', () => {
    expect(connectionCanReuseToOtherAgents({
      source: 'provider',
      id: '',
      agentId: 'claude',
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

  it('hides adapter-generated providers (projections are not tickets)', () => {
    expect(shouldShowReuseAction(kimiEntry, {
      reuseEnabled: true,
      adapterGeneratedProviderIds: new Set(['kimi-1']),
    })).toBe(false);
  });

  it('shows Kimi membership when reuse is wired', () => {
    expect(shouldShowReuseAction(kimiEntry, { reuseEnabled: true })).toBe(true);
  });

  it('shows account rows when reuse is wired (true tickets)', () => {
    const oauth = account({ id: 'claude-oauth', kind: 'oauth' });
    expect(shouldShowReuseAction({
      source: 'account',
      id: oauth.id,
      agentId: oauth.agentId,
    }, { reuseEnabled: true })).toBe(true);
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

  it('shows unknown / non-whitelist providers when reuse is wired', () => {
    const unknown = provider({
      id: 'relay-1',
      agentId: 'claude',
      preset: 'custom',
      configText: '{}',
    });
    expect(shouldShowReuseAction({
      source: 'provider',
      id: unknown.id,
      agentId: unknown.agentId,
      provider: unknown,
    }, { reuseEnabled: true })).toBe(true);
  });
});
