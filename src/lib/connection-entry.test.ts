import { describe, expect, it } from 'vitest';
import type { Account, Provider } from '@/lib/types';
import { accountToEntry, mergeConnectionEntries, providerToEntry } from './connection-entry';

function acc(partial: Partial<Account> & Pick<Account, 'id' | 'kind' | 'label'>): Account {
  return {
    agentId: 'claude',
    isCurrent: false,
    tokenValid: true,
    ...partial,
  };
}

function prov(partial: Partial<Provider> & Pick<Provider, 'id' | 'name'>): Provider {
  return {
    agentId: 'claude',
    preset: 'custom',
    configText: JSON.stringify({
      env: {
        ANTHROPIC_BASE_URL: 'https://relay.example.com',
        ANTHROPIC_AUTH_TOKEN: '***',
      },
    }),
    configFormat: 'json',
    isCurrent: false,
    ...partial,
  };
}

describe('connection-entry', () => {
  it('maps official vs custom provider endpoints', () => {
    const custom = providerToEntry(prov({ id: 'p1', name: 'Relay' }));
    expect(custom.endpointMode).toBe('custom');
    expect(custom.endpointHost).toContain('relay.example.com');

    const flaggedOfficial = providerToEntry(prov({
      id: 'p2',
      name: 'Anthropic',
      official: true,
      configText: JSON.stringify({
        env: { ANTHROPIC_BASE_URL: 'https://api.anthropic.com' },
      }),
    }));
    expect(flaggedOfficial.endpointMode).toBe('official');

    const inferredOfficial = providerToEntry(prov({
      id: 'p3',
      name: 'Anthropic',
      preset: 'anthropic',
      configText: JSON.stringify({
        env: { ANTHROPIC_BASE_URL: 'https://api.anthropic.com' },
      }),
    }));
    expect(inferredOfficial.endpointMode).toBe('official');

    const inferredCustom = providerToEntry(prov({
      id: 'p4',
      name: 'Relay',
      preset: 'anthropic',
      configText: JSON.stringify({
        env: { ANTHROPIC_BASE_URL: 'https://relay.example.com' },
      }),
    }));
    expect(inferredCustom.endpointMode).toBe('custom');
  });

  it('maps API key accounts as official-mode rows', () => {
    const row = accountToEntry(acc({ id: 'a1', kind: 'apikey', label: 'Key' }));
    expect(row.source).toBe('account');
    expect(row.kind).toBe('apikey');
    expect(row.endpointMode).toBe('official');
    expect(row.title).toBe('Key');
  });

  it('maps a ZCode API Key with a custom URL as a custom endpoint', () => {
    const row = accountToEntry(acc({
      id: 'z1',
      agentId: 'zcode',
      kind: 'apikey',
      label: 'grok',
      endpoint: 'https://api.qooo.io/v1',
    }));
    expect(row.endpointMode).toBe('custom');
    expect(row.endpointHost).toContain('api.qooo.io');
  });

  it('sorts current rows first when merging pools', () => {
    const rows = mergeConnectionEntries(
      [acc({ id: 'a1', kind: 'oauth', label: 'old', isCurrent: false, updatedAt: '1' })],
      [prov({ id: 'p1', name: 'now', isCurrent: true, updatedAt: '0' })],
    );
    expect(rows[0].id).toBe('p1');
    expect(rows[0].isCurrent).toBe(true);
  });
});
