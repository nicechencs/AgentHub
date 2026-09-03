import { describe, expect, it } from 'vitest';
import type { DefaultRoutePoolOverview } from '@/lib/backend/contracts/adapter';
import {
  importedSourceKeys,
  isPoolShareableLogin,
  resolveTicketPoolImportAction,
  ticketPoolImportKey,
  ticketPoolImportMenuState,
} from './ticket-pool-import';

describe('ticketPoolImportKey', () => {
  it('joins source kind and id the same way pool membership does', () => {
    expect(ticketPoolImportKey({ sourceKind: 'account', sourceId: 'acc-1' })).toBe('account:acc-1');
    expect(ticketPoolImportKey({ sourceKind: 'provider', sourceId: 'p-1' })).toBe('provider:p-1');
  });
});

describe('importedSourceKeys', () => {
  it('collects membership across default pools', () => {
    const pools: DefaultRoutePoolOverview[] = [
      {
        id: 'pool-codex',
        targetAgentId: 'codex',
        surface: 'responses',
        dialect: 'codex',
        unifiedGatewayEnrolled: false,
        members: [{ sourceKind: 'account', sourceId: 'acc-1', enabled: true }],
      },
      {
        id: 'pool-claude',
        targetAgentId: 'claude',
        surface: 'messages',
        dialect: 'claude',
        unifiedGatewayEnrolled: false,
        members: [{ sourceKind: 'provider', sourceId: 'p-1', enabled: true }],
      },
    ];
    expect([...importedSourceKeys(pools)].sort()).toEqual(['account:acc-1', 'provider:p-1']);
  });
});

describe('isPoolShareableLogin', () => {
  it('allows API keys from any Agent, including WorkBuddy / ZCode / Pi', () => {
    expect(isPoolShareableLogin({ agentId: 'workbuddy', credentialClass: 'api_key' })).toBe(true);
    expect(isPoolShareableLogin({ agentId: 'zcode', kind: 'apikey' })).toBe(true);
    expect(isPoolShareableLogin({ agentId: 'pi', credentialClass: 'api_key' })).toBe(true);
    expect(isPoolShareableLogin({ agentId: 'kimi', credentialClass: 'api_key' })).toBe(true);
  });

  it('allows Claude / Codex / Grok official logins and blocks other OAuth', () => {
    expect(isPoolShareableLogin({ agentId: 'claude', credentialClass: 'oauth' })).toBe(true);
    expect(isPoolShareableLogin({ agentId: 'codex', kind: 'oauth' })).toBe(true);
    expect(isPoolShareableLogin({ agentId: 'kimi', credentialClass: 'oauth' })).toBe(false);
    expect(isPoolShareableLogin({ agentId: 'workbuddy', kind: 'oauth' })).toBe(false);
  });
});

describe('resolveTicketPoolImportAction', () => {
  it('disables when the connection pool is off', () => {
    expect(resolveTicketPoolImportAction(
      { agentId: 'kimi', credentialClass: 'api_key' },
      { poolEnabled: false, alreadyImported: false },
    )).toEqual({ disabled: true, reason: '连接池还没开启' });
  });

  it('disables official logins that cannot be shared', () => {
    expect(resolveTicketPoolImportAction(
      { agentId: 'kimi', credentialClass: 'oauth' },
      { poolEnabled: true, alreadyImported: false },
    )).toEqual({ disabled: true, reason: '这份登录目前不能分享至连接池' });
  });

  it('disables a login already in the pool', () => {
    expect(resolveTicketPoolImportAction(
      { agentId: 'kimi', credentialClass: 'api_key' },
      { poolEnabled: true, alreadyImported: true },
    )).toEqual({ disabled: true, reason: '已在连接池' });
  });

  it('enables an API key that is not in the pool, including WorkBuddy', () => {
    expect(resolveTicketPoolImportAction(
      { agentId: 'workbuddy', credentialClass: 'api_key' },
      { poolEnabled: true, alreadyImported: false },
    )).toEqual({ disabled: false });
  });
});

describe('ticketPoolImportMenuState', () => {
  it('keeps the disable reason when this login cannot join', () => {
    expect(ticketPoolImportMenuState(
      { disabled: true, reason: '这份登录目前不能分享至连接池' },
      null,
      'provider:kimi-1',
    )).toEqual({
      disabled: true,
      reason: '这份登录目前不能分享至连接池',
      busy: false,
    });
  });

  it('shows 已在连接池 as the disable reason', () => {
    expect(ticketPoolImportMenuState(
      { disabled: true, reason: '已在连接池' },
      null,
      'provider:kimi-1',
    )).toEqual({
      disabled: true,
      reason: '已在连接池',
      busy: false,
    });
  });

  it('stays enabled when the login can join', () => {
    expect(ticketPoolImportMenuState(
      { disabled: false },
      null,
      'provider:kimi-1',
    )).toEqual({ disabled: false, reason: undefined, busy: false });
  });

  it('shows 分享中… while this login is importing', () => {
    expect(ticketPoolImportMenuState(
      { disabled: false },
      'provider:kimi-1',
      'provider:kimi-1',
    )).toEqual({
      disabled: true,
      reason: '分享中…',
      busy: true,
    });
  });
});
