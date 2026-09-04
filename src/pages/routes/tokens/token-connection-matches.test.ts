import { describe, expect, it } from 'vitest';
import type { Account, Provider } from '@/lib/types';
import {
  connectionMatchAgentNames,
  hashLocalToken,
  matchesConnectionEntryKeys,
} from './token-connection-matches';

function provider(partial: Partial<Provider> & Pick<Provider, 'id'>): Provider {
  return {
    agentId: 'claude',
    name: partial.name ?? partial.id,
    preset: 'custom',
    configText: '{}',
    configFormat: 'json',
    isCurrent: false,
    ...partial,
  };
}

function account(partial: Partial<Account> & Pick<Account, 'id'>): Account {
  return {
    agentId: 'claude',
    kind: 'apikey',
    label: partial.label ?? partial.id,
    isCurrent: false,
    tokenValid: true,
    status: 'active',
    createdAt: '',
    updatedAt: '',
    ...partial,
  };
}

describe('hashLocalToken', () => {
  it('hashes the trimmed token as lowercase sha-256 hex', async () => {
    const hash = await hashLocalToken('  ahb_fixture\n');
    expect(hash).toHaveLength(64);
    expect(hash).toBe(await hashLocalToken('ahb_fixture'));
    expect(hash).not.toBe(await hashLocalToken('ahb_other'));
    expect(await hashLocalToken('   ')).toBe('');
  });
});

describe('matchesConnectionEntryKeys', () => {
  const tokenHash = 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa';

  it('keeps Connections API Key rows with the same hash', () => {
    const matches = matchesConnectionEntryKeys({
      tokenHash,
      providers: [
        provider({ id: 'p-hit', name: 'Local Claude', secretHash: tokenHash }),
        provider({ id: 'p-miss', secretHash: 'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb' }),
        provider({ id: 'p-pool', secretHash: tokenHash, home: 'route_pool' }),
        provider({
          id: 'agenthub_claude_bridge',
          name: '本机路由',
          secretHash: tokenHash,
        }),
      ],
      accounts: [
        account({ id: 'a-hit', label: 'Key', secretHash: tokenHash.toUpperCase() }),
        account({ id: 'a-oauth', kind: 'oauth', secretHash: tokenHash }),
      ],
    });
    expect(matches.map((row) => `${row.sourceKind}:${row.sourceId}`)).toEqual([
      'provider:p-hit',
      'account:a-hit',
    ]);
  });

  it('returns nothing without a hash', () => {
    expect(matchesConnectionEntryKeys({
      tokenHash: '',
      providers: [provider({ id: 'p', secretHash: tokenHash })],
    })).toEqual([]);
  });
});

describe('connectionMatchAgentNames', () => {
  it('dedupes display names in match order', () => {
    expect(connectionMatchAgentNames(
      [
        { sourceKind: 'provider', sourceId: 'p1', agentId: 'claude', label: 'a' },
        { sourceKind: 'provider', sourceId: 'p2', agentId: 'claude', label: 'b' },
        { sourceKind: 'account', sourceId: 'a1', agentId: 'codex', label: 'c' },
      ],
      (id) => (id === 'claude' ? 'Claude' : 'Codex'),
    )).toEqual(['Claude', 'Codex']);
  });
});
