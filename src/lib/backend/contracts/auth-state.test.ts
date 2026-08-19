import { describe, expect, it } from 'vitest';
import { authDisplayForAccount, authDisplayForAgentStatus } from './auth-state';
import type { Account, AgentStatus } from '@/lib/types';

function account(overrides: Partial<Account> = {}): Account {
  return {
    id: 'a1',
    agentId: 'claude',
    kind: 'oauth',
    label: 'user@example.com',
    isCurrent: true,
    tokenValid: true,
    ...overrides,
  };
}

describe('auth state display mapping', () => {
  it.each([
    ['verified', '已验证'],
    ['renewable', '可续期'],
    ['configured', '已配置'],
    ['needs_login', '需要重新登录'],
    ['unknown', '状态未知'],
    ['missing', '未登录'],
  ] as const)('renders the six semantic labels: %s', (health, label) => {
    const display = authDisplayForAccount(account({ authHealth: health }));
    expect(display.health).toBe(health);
    expect(display.label).toBe(label);
  });

  it('does not call refresh-token credentials verified', () => {
    expect(authDisplayForAccount(account({ refreshable: true })).label).toBe('可续期');
    expect(
      authDisplayForAccount(
        account({ kind: 'apikey', authHealth: undefined, tokenValid: true }),
      ).label,
    ).toBe('已配置');
  });

  it('keeps an explicit unknown health out of the expired visual state', () => {
    const display = authDisplayForAccount(
      account({ authHealth: 'unknown', tokenRemainingSec: -60, refreshable: false }),
    );

    expect(display).toMatchObject({ health: 'unknown', label: '状态未知', legacyStatus: 'expiring' });
  });

  it('accepts old AgentStatus rows and keeps renewable rows healthy', () => {
    const status: AgentStatus = {
      agentId: 'grok',
      installed: true,
      authStatus: 'valid',
      authLabel: '已登录',
      authHealth: 'renewable',
      running: false,
    };
    expect(authDisplayForAgentStatus(status)).toMatchObject({
      health: 'renewable',
      label: '可续期',
      legacyStatus: 'valid',
    });
  });
});
