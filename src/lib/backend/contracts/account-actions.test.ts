import { describe, expect, it } from 'vitest';
import {
  accountActionPolicy,
  oauthListAction,
  oauthListActionProbesQuota,
} from './account-actions';
import type { Account } from '@/lib/types';

function account(overrides: Partial<Account> = {}): Pick<
  Account,
  'agentId' | 'kind' | 'provider' | 'refreshable' | 'source' | 'isCurrent'
> {
  return {
    agentId: 'claude',
    kind: 'oauth',
    provider: undefined,
    refreshable: true,
    isCurrent: false,
    ...overrides,
  };
}

describe('account action policy', () => {
  it.each(['kimi', 'codex', 'claude', 'grok'] as const)('hides refresh for %s', (agentId) => {
    expect(accountActionPolicy(account({ agentId }))).toBeUndefined();
  });

  // Canonical keys + aliases from Rust PI_PROVIDER_SPECS.refreshable.
  it.each(['anthropic', 'claude', 'openai', 'openai-codex', 'codex', 'xai', 'grok'] as const)(
    'allows Pi credential refresh for %s',
    (provider) => {
      expect(accountActionPolicy(account({ agentId: 'pi', provider }))).toEqual({
        kind: 'refresh-credentials',
        label: '刷新登录信息',
      });
    },
  );

  it('requires refresh credentials and hides unsupported Pi providers/non-OAuth', () => {
    expect(accountActionPolicy(account({ agentId: 'pi', provider: 'openai', refreshable: false }))).toBeUndefined();
    expect(accountActionPolicy(account({ agentId: 'pi', provider: 'google' }))).toBeUndefined();
    expect(accountActionPolicy(account({ agentId: 'pi', provider: 'openrouter' }))).toBeUndefined();
    expect(accountActionPolicy(account({ agentId: 'pi', kind: 'apikey', provider: 'xai' }))).toBeUndefined();
  });
});

describe('oauthListAction', () => {
  it('rotates Hub-owned Grok refresh tokens', () => {
    expect(oauthListAction(account({
      agentId: 'grok',
      source: 'oauth_pkce',
    }))).toEqual({
      kind: 'refresh-credentials',
      label: '刷新',
    });
    expect(oauthListAction(account({
      agentId: 'grok',
      source: 'oauth_refresh',
      refreshable: true,
    }))).toEqual({
      kind: 'refresh-credentials',
      label: '刷新',
    });
  });

  it('syncs current CLI-owned Grok and quotas unused imported rows', () => {
    expect(oauthListAction(account({
      agentId: 'grok',
      source: 'live',
      isCurrent: true,
    }))).toEqual({
      kind: 'sync-current-login',
      label: '同步当前登录',
    });
    expect(oauthListAction(account({
      agentId: 'grok',
      source: 'auth.json',
      isCurrent: true,
    }))).toEqual({
      kind: 'sync-current-login',
      label: '同步当前登录',
    });
    expect(oauthListAction(account({ agentId: 'grok', source: 'live' }))).toEqual({
      kind: 'refresh-quota',
      label: '刷新',
    });
  });

  it('keeps Pi credential refresh', () => {
    expect(oauthListAction(account({ agentId: 'pi', provider: 'anthropic' }))?.kind).toBe(
      'refresh-credentials',
    );
  });

  it('rotates Hub-owned Codex refresh tokens', () => {
    expect(oauthListAction(account({
      agentId: 'codex',
      source: 'oauth_pkce',
    }))).toEqual({
      kind: 'refresh-credentials',
      label: '刷新',
    });
  });

  it('syncs current CLI-owned Codex and quotas unused imported rows', () => {
    expect(oauthListAction(account({
      agentId: 'codex',
      source: 'live',
      isCurrent: true,
    }))).toEqual({
      kind: 'sync-current-login',
      label: '同步当前登录',
    });
    expect(oauthListAction(account({ agentId: 'codex', source: 'live' }))).toEqual({
      kind: 'refresh-quota',
      label: '刷新',
    });
  });

  it('falls back to quota refresh for Claude', () => {
    expect(oauthListAction(account({ agentId: 'claude' }))).toEqual({
      kind: 'refresh-quota',
      label: '刷新',
    });
  });

  it('hides Kimi CLI-owned OAuth', () => {
    expect(oauthListAction(account({ agentId: 'kimi' }))).toBeUndefined();
  });

  it('probes quota after every visible list-row action, including current Codex sync', () => {
    expect(oauthListActionProbesQuota('refresh-quota')).toBe(true);
    expect(oauthListActionProbesQuota('refresh-credentials')).toBe(true);
    expect(oauthListActionProbesQuota('sync-current-login')).toBe(true);
  });
});
