import { describe, expect, it } from 'vitest';
import type { Account, Provider } from '@/lib/types';
import {
  accountToEntry,
  countByKind,
  filterConnectionEntries,
  mergeConnectionEntries,
  providerToEntry,
} from './connection-model';

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

describe('connection-model', () => {
  it('maps oauth / apikey; providers collapse into apikey kind', () => {
    expect(accountToEntry(acc({ id: 'a1', kind: 'oauth', label: 'me@x.com' })).kind).toBe(
      'oauth',
    );
    expect(accountToEntry(acc({ id: 'a2', kind: 'apikey', label: 'sk-…' })).kind).toBe(
      'apikey',
    );
    const p = providerToEntry(prov({ id: 'p1', name: 'relay' }));
    expect(p.kind).toBe('apikey');
    expect(p.source).toBe('provider');
    expect(p.endpointMode).toBe('custom');
  });

  it('marks official providers', () => {
    const p = providerToEntry(
      prov({
        id: 'p-off',
        name: 'Anthropic',
        official: true,
        configText: JSON.stringify({ env: {}, model: 'sonnet' }),
      }),
    );
    expect(p.endpointMode).toBe('official');
  });

  it('merges pools with current first then newer updatedAt', () => {
    const rows = mergeConnectionEntries(
      [
        acc({
          id: 'old',
          kind: 'oauth',
          label: 'old',
          updatedAt: '2026-01-01 00:00:00',
        }),
        acc({
          id: 'cur',
          kind: 'apikey',
          label: 'key',
          isCurrent: true,
          updatedAt: '2026-01-02 00:00:00',
        }),
      ],
      [
        prov({
          id: 'p-new',
          name: 'new-relay',
          updatedAt: '2026-06-01 00:00:00',
        }),
      ],
    );
    expect(rows.map((r) => r.id)).toEqual(['cur', 'p-new', 'old']);
    expect(rows[0]!.isCurrent).toBe(true);
  });

  it('filters and counts: provider rows count as apikey', () => {
    const rows = mergeConnectionEntries(
      [
        acc({ id: 'o', kind: 'oauth', label: 'o' }),
        acc({ id: 'k', kind: 'apikey', label: 'k' }),
      ],
      [prov({ id: 'p', name: 'p' })],
    );
    const counts = countByKind(rows);
    expect(counts).toEqual({ all: 3, oauth: 1, apikey: 2 });
    expect(filterConnectionEntries(rows, 'apikey')).toHaveLength(2);
    expect(filterConnectionEntries(rows, 'oauth')[0]!.id).toBe('o');
  });
});
