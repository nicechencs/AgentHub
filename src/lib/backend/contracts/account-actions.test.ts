import { describe, expect, it } from 'vitest';
import { accountActionPolicy } from './account-actions';
import type { Account } from '@/lib/types';

function account(overrides: Partial<Account> = {}): Pick<
  Account,
  'agentId' | 'kind' | 'provider' | 'refreshable'
> {
  return {
    agentId: 'claude',
    kind: 'oauth',
    provider: undefined,
    refreshable: true,
    ...overrides,
  };
}

describe('account action policy', () => {
  it('keeps Grok sync-current-login and never exposes generic refresh wording', () => {
    expect(accountActionPolicy(account({ agentId: 'grok' }))).toEqual({
      kind: 'sync-current-login',
      label: '同步当前登录',
    });
  });

  it.each(['kimi', 'codex', 'claude'] as const)('hides refresh for %s', (agentId) => {
    expect(accountActionPolicy(account({ agentId }))).toBeUndefined();
  });

  it.each(['anthropic', 'openai', 'openai-codex', 'xai'] as const)(
    'allows Pi credential refresh for %s',
    (provider) => {
      expect(accountActionPolicy(account({ agentId: 'pi', provider }))).toEqual({
        kind: 'refresh-credentials',
        label: '刷新凭据',
      });
    },
  );

  it('requires refresh credentials and hides unsupported Pi providers/non-OAuth', () => {
    expect(accountActionPolicy(account({ agentId: 'pi', provider: 'openai', refreshable: false }))).toBeUndefined();
    expect(accountActionPolicy(account({ agentId: 'pi', provider: 'google' }))).toBeUndefined();
    expect(accountActionPolicy(account({ agentId: 'pi', kind: 'apikey', provider: 'xai' }))).toBeUndefined();
  });
});
