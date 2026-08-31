import { describe, expect, it, vi } from 'vitest';
import { createTranslator } from '@/lib/i18n';
import type { Account } from '@/lib/types';
import type { LiveAuthProbe } from '@/lib/backend/contracts/ports';
import {
  poolAuthorizationRefreshAction,
  poolAuthorizationRefreshLabels,
  runPoolAuthorizationRefresh,
  type PoolAuthorizationRefreshDeps,
} from './pool-authorization-refresh';

const t = createTranslator('zh');

function account(partial: Partial<Account> = {}): Account {
  return {
    id: 'acc-1',
    agentId: 'claude',
    kind: 'oauth',
    label: 'user@example.com',
    isCurrent: false,
    tokenValid: true,
    refreshable: true,
    ...partial,
  };
}

function deps(partial: Partial<PoolAuthorizationRefreshDeps> = {}): PoolAuthorizationRefreshDeps {
  return {
    probeLiveAuth: vi.fn(),
    importCurrentLogin: vi.fn(),
    refreshToken: vi.fn(),
    refreshQuota: vi.fn(),
    ...partial,
  };
}

describe('poolAuthorizationRefreshAction', () => {
  it('hides refresh without an account or for API keys', () => {
    expect(poolAuthorizationRefreshAction(null)).toBeUndefined();
    expect(poolAuthorizationRefreshAction(account({ kind: 'apikey' }))).toBeUndefined();
  });

  it('uses the shared OAuth list action', () => {
    expect(poolAuthorizationRefreshAction(account({ agentId: 'claude' }))).toEqual({
      kind: 'refresh-quota',
      label: '刷新',
    });
    expect(poolAuthorizationRefreshAction(account({
      agentId: 'grok',
      source: 'oauth_pkce',
    }))).toEqual({
      kind: 'refresh-credentials',
      label: '刷新',
    });
  });
});

describe('poolAuthorizationRefreshLabels', () => {
  it('labels quota/credential refresh as 刷新', () => {
    expect(poolAuthorizationRefreshLabels({ kind: 'refresh-quota', label: '刷新' }, t))
      .toEqual({ idle: '刷新', busy: '刷新中…' });
  });

  it('labels sync-current-login as 同步当前登录', () => {
    expect(poolAuthorizationRefreshLabels({ kind: 'sync-current-login', label: '同步当前登录' }, t))
      .toEqual({ idle: '同步当前登录', busy: '同步中…' });
  });
});

describe('runPoolAuthorizationRefresh', () => {
  it('probes quota for Claude', async () => {
    const refreshQuota = vi.fn().mockResolvedValue(account());
    const result = await runPoolAuthorizationRefresh(account(), t, deps({ refreshQuota }));
    expect(refreshQuota).toHaveBeenCalledWith('claude', 'acc-1');
    expect(result).toEqual({
      toast: { title: '已刷新', variant: 'success' },
      reload: true,
    });
  });

  it('rotates Hub-owned credentials then probes quota', async () => {
    const refreshToken = vi.fn().mockResolvedValue(undefined);
    const refreshQuota = vi.fn().mockRejectedValue(new Error('quota down'));
    const result = await runPoolAuthorizationRefresh(
      account({ agentId: 'grok', source: 'oauth_pkce' }),
      t,
      deps({ refreshToken, refreshQuota }),
    );
    expect(refreshToken).toHaveBeenCalledWith('grok', 'acc-1');
    expect(refreshQuota).toHaveBeenCalledWith('grok', 'acc-1');
    expect(result.reload).toBe(true);
    expect(result.toast.title).toBe('已刷新');
  });

  it('imports the current login then probes quota', async () => {
    const probe: LiveAuthProbe = {
      agentId: 'codex',
      kind: 'oauth',
      summary: 'logged in',
      hasCredentials: true,
    };
    const probeLiveAuth = vi.fn().mockResolvedValue(probe);
    const importCurrentLogin = vi.fn().mockResolvedValue(account({
      id: 'imported',
      agentId: 'codex',
      label: 'codex-user',
    }));
    const refreshQuota = vi.fn().mockResolvedValue(undefined);
    const result = await runPoolAuthorizationRefresh(
      account({ agentId: 'codex', source: 'live', isCurrent: true }),
      t,
      deps({ probeLiveAuth, importCurrentLogin, refreshQuota }),
    );
    expect(importCurrentLogin).toHaveBeenCalledWith('codex');
    expect(refreshQuota).toHaveBeenCalledWith('codex', 'imported');
    expect(result.toast.title).toBe('已导入授权');
    expect(result.reload).toBe(true);
  });

  it('keeps a partial refresh when client files still need a sync', async () => {
    const error = new Error('登录已刷新，但还没写进客户端文件，请再同步一次');
    error.name = 'OauthFileSyncPending';
    const result = await runPoolAuthorizationRefresh(
      account({ agentId: 'grok', source: 'oauth_pkce' }),
      t,
      deps({ refreshToken: vi.fn().mockRejectedValue(error) }),
    );
    expect(result).toEqual({
      toast: {
        title: '登录已刷新，客户端文件还没写上',
        description: '登录已刷新，但还没写进客户端文件，请再同步一次',
        variant: 'danger',
      },
      reload: true,
    });
  });
});
