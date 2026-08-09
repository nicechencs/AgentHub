import { describe, expect, it } from 'vitest';
import type { Account, Provider } from '@/lib/types';
import {
  accountToEntry,
  authStatusOfAccount,
  countByKind,
  deleteConnectionDialogDescription,
  deleteConnectionToastDescription,
  filterConnectionEntries,
  liveApiKeyImportGate,
  liveAuthImportGate,
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
  it('surfaces account email / subscription on oauth entries', () => {
    const entry = accountToEntry(
      acc({
        id: 'o1',
        kind: 'oauth',
        label: 'me@x.com',
        email: 'me@x.com',
        identityLabel: 'me@x.com',
        subscription: 'plus',
        isCurrent: false,
        source: 'oauth_pkce',
      }),
    );
    expect(entry.title).toBe('me@x.com');
    expect(entry.identityLabel).toBe('me@x.com');
    expect(entry.subscription).toBe('plus');
    expect(entry.subtitle).toContain('plus');
    expect(entry.subtitle).not.toContain('oauth_pkce');
  });

  it('treats oauth without remaining as valid, not none', () => {
    expect(
      authStatusOfAccount(
        acc({ id: 'o', kind: 'oauth', label: 'x', tokenValid: true }),
      ),
    ).toBe('valid');
    expect(
      authStatusOfAccount(
        acc({
          id: 'o2',
          kind: 'oauth',
          label: 'x',
          tokenValid: true,
          tokenRemainingSec: -10,
        }),
      ),
    ).toBe('expired');
    expect(
      authStatusOfAccount(
        acc({
          id: 'o3',
          kind: 'oauth',
          label: 'x',
          tokenValid: true,
          tokenRemainingSec: 2 * 3600,
        }),
      ),
    ).toBe('expiring');
  });

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

  it('uses the same recoverable delete semantics for current accounts and providers', () => {
    for (const current of [true, false]) {
      const dialog = deleteConnectionDialogDescription({ isCurrent: current });
      const toast = deleteConnectionToastDescription({ isCurrent: current });
      expect(dialog).toContain('移入回收站');
      expect(dialog).toContain(current ? '当前连接可能仍继续生效' : '不会修改本机配置文件');
      expect(toast).toContain('已移入回收站');
      expect(toast).toContain(current ? '当前连接可能仍继续生效' : '本机配置未修改');
    }
  });

  it('only enables current-login import for credentialed OAuth/file-auth probes', () => {
    expect(liveAuthImportGate(undefined, true)).toEqual({
      enabled: false,
      reason: '正在检测本机登录态…',
    });
    expect(
      liveAuthImportGate({ kind: 'api_key', hasCredentials: true }, false).reason,
    ).toContain('API Key');
    expect(
      liveAuthImportGate({ kind: 'desktop-login', hasCredentials: true }, false).enabled,
    ).toBe(false);
    expect(liveAuthImportGate({ kind: 'oauth', hasCredentials: false }, false).enabled).toBe(
      false,
    );
    expect(liveAuthImportGate({ kind: 'oauth', hasCredentials: true }, false)).toEqual({
      enabled: true,
      reason: '',
    });
    expect(liveAuthImportGate({ kind: 'file-auth.json', hasCredentials: true }, false)).toEqual({
      enabled: true,
      reason: '',
    });
  });

  it('only enables API Key import for credentialed API-key probes', () => {
    expect(liveApiKeyImportGate(undefined, true)).toEqual({
      enabled: false,
      reason: '正在检测本机认证方式…',
    });
    expect(liveApiKeyImportGate({ kind: 'oauth', hasCredentials: true }, false)).toEqual({
      enabled: false,
      reason: '当前本机为 OAuth 登录态，请导入当前登录态',
    });
    expect(liveApiKeyImportGate({ kind: 'api_key', hasCredentials: false }, false).enabled).toBe(
      false,
    );
    expect(liveApiKeyImportGate({ kind: 'api_key', hasCredentials: true }, false)).toEqual({
      enabled: true,
      reason: '',
    });
  });
});
