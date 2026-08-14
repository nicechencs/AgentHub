import { describe, expect, it } from 'vitest';
import type { Account, Provider } from '@/lib/types';
import type { AdapterProfile, AdapterProfileStatus } from '@/lib/api/adapter';
import { computeConnectionUsageMap } from './connection-usage';
import { connectSourceKey } from './types';

function account(overrides: Partial<Account> = {}): Account {
  return {
    id: 'acc-1',
    agentId: 'claude',
    kind: 'oauth',
    label: 'claude@example.com',
    isCurrent: false,
    tokenValid: true,
    ...overrides,
  };
}

function provider(overrides: Partial<Provider> = {}): Provider {
  return {
    id: 'prov-1',
    agentId: 'kimi',
    name: 'Kimi Member',
    preset: 'kimi-code-membership',
    configText: '{}',
    configFormat: 'json',
    isCurrent: false,
    ...overrides,
  };
}

function profile(overrides: Partial<AdapterProfile> = {}): AdapterProfile {
  return {
    id: 'profile-1',
    name: 'kimi → claude',
    sourceKind: 'provider',
    sourceId: 'kimi-src',
    targetAgentId: 'claude',
    route: 'native_endpoint',
    mode: 'api',
    status: 'active',
    ruleId: 'rule',
    ruleVersion: '1',
    generatedProviderId: 'gen-claude',
    autoStart: false,
    createdAt: '2026-08-01T00:00:00.000Z',
    updatedAt: '2026-08-01T00:00:00.000Z',
    ...overrides,
  };
}

describe('computeConnectionUsageMap', () => {
  it('records direct usage from isCurrent credentials', () => {
    const map = computeConnectionUsageMap({
      accounts: [account({ id: 'acc-cur', isCurrent: true }), account({ id: 'acc-idle' })],
      providers: [provider({ id: 'prov-cur', agentId: 'kimi', isCurrent: true })],
      profiles: [],
      poolComplete: true,
    });

    expect(map.get(connectSourceKey({ kind: 'account', id: 'acc-cur' }))).toEqual({
      status: 'known',
      agents: [{ agentId: 'claude', via: 'direct' }],
    });
    expect(map.get(connectSourceKey({ kind: 'account', id: 'acc-idle' }))).toEqual({
      status: 'known',
      agents: [],
    });
    expect(map.get(connectSourceKey({ kind: 'provider', id: 'prov-cur' }))).toEqual({
      status: 'known',
      agents: [{ agentId: 'kimi', via: 'direct' }],
    });
  });

  it('records adapter usage only when the generated Provider is current', () => {
    const source = provider({ id: 'kimi-src', agentId: 'kimi' });
    const generatedCurrent = provider({ id: 'gen-claude', agentId: 'claude', isCurrent: true });
    const map = computeConnectionUsageMap({
      accounts: [],
      providers: [source, generatedCurrent],
      profiles: [profile()],
      poolComplete: true,
    });

    expect(map.get(connectSourceKey({ kind: 'provider', id: 'kimi-src' }))).toEqual({
      status: 'known',
      agents: [{ agentId: 'claude', via: 'adapter' }],
    });
    expect(map.has(connectSourceKey({ kind: 'provider', id: 'gen-claude' }))).toBe(false);
  });

  it('does not count adapter usage when the generated Provider is not current', () => {
    const map = computeConnectionUsageMap({
      accounts: [],
      providers: [
        provider({ id: 'kimi-src' }),
        provider({ id: 'gen-claude', agentId: 'claude', isCurrent: false }),
      ],
      profiles: [profile()],
      poolComplete: true,
    });
    expect(map.get(connectSourceKey({ kind: 'provider', id: 'kimi-src' }))).toEqual({
      status: 'known',
      agents: [],
    });
  });

  it('dedupes the same Agent with direct winning over adapter', () => {
    const map = computeConnectionUsageMap({
      accounts: [account({ id: 'acc-src', agentId: 'claude', isCurrent: true })],
      providers: [provider({ id: 'gen-claude', agentId: 'claude', isCurrent: true })],
      profiles: [profile({ sourceKind: 'account', sourceId: 'acc-src' })],
      poolComplete: true,
    });
    expect(map.get(connectSourceKey({ kind: 'account', id: 'acc-src' }))).toEqual({
      status: 'known',
      agents: [{ agentId: 'claude', via: 'direct' }],
    });
  });

  it('marks incomplete when the generated Provider is missing', () => {
    const map = computeConnectionUsageMap({
      accounts: [],
      providers: [provider({ id: 'kimi-src' })],
      profiles: [profile({ generatedProviderId: 'missing-gen' })],
      poolComplete: true,
    });
    expect(map.get(connectSourceKey({ kind: 'provider', id: 'kimi-src' }))).toEqual({
      status: 'incomplete',
      agents: [],
    });
  });

  it('marks incomplete when the profile source is missing', () => {
    const map = computeConnectionUsageMap({
      accounts: [],
      providers: [provider({ id: 'gen-claude', agentId: 'claude', isCurrent: true })],
      profiles: [profile({ sourceId: 'missing-src' })],
      poolComplete: true,
    });
    expect(map.get(connectSourceKey({ kind: 'provider', id: 'missing-src' }))).toEqual({
      status: 'incomplete',
      agents: [],
    });
    expect(map.has(connectSourceKey({ kind: 'provider', id: 'gen-claude' }))).toBe(false);
  });

  it('marks every row incomplete when the pool is only partially loaded (never unused)', () => {
    const map = computeConnectionUsageMap({
      accounts: [account({ id: 'idle', isCurrent: false })],
      providers: [],
      profiles: [],
      poolComplete: false,
    });
    expect(map.get(connectSourceKey({ kind: 'account', id: 'idle' }))).toEqual({
      status: 'incomplete',
      agents: [],
    });
  });

  it.each<AdapterProfileStatus>(['applying', 'active', 'needs_attention'])(
    'counts profile status %s when the generated Provider is current',
    (status) => {
      const map = computeConnectionUsageMap({
        accounts: [],
        providers: [
          provider({ id: 'kimi-src' }),
          provider({ id: 'gen-claude', agentId: 'claude', isCurrent: true }),
        ],
        profiles: [profile({ status })],
        poolComplete: true,
      });
      expect(map.get(connectSourceKey({ kind: 'provider', id: 'kimi-src' }))).toEqual({
        status: 'known',
        agents: [{ agentId: 'claude', via: 'adapter' }],
      });
    },
  );
});
