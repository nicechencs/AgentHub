import { describe, expect, it } from 'vitest';
import {
  canAutoImportProbe,
  planLocalLoginAutoImport,
  resolveAutoImportLocalLogin,
  shouldAutoImportDiscoveredLogin,
  showConnectionsImportLoginAction,
} from './local-login-auto-import';
import { ticketAddActionsForAgent, buildTicketAddMenu } from './ticket-add-menu';

describe('auto-import this computer login preference', () => {
  it('defaults on when the stored value is missing', () => {
    expect(resolveAutoImportLocalLogin(undefined)).toBe(true);
    expect(resolveAutoImportLocalLogin(null)).toBe(true);
    expect(resolveAutoImportLocalLogin(true)).toBe(true);
    expect(resolveAutoImportLocalLogin(false)).toBe(false);
  });

  it('hides the Connections import action while auto-import is on', () => {
    expect(showConnectionsImportLoginAction(true)).toBe(false);
    expect(showConnectionsImportLoginAction(undefined)).toBe(false);
    expect(showConnectionsImportLoginAction(false)).toBe(true);
  });

  it('auto-imports only when the preference is on and discovery found a login', () => {
    expect(shouldAutoImportDiscoveredLogin(true, 'account')).toBe(true);
    expect(shouldAutoImportDiscoveredLogin(true, 'provider')).toBe(true);
    expect(shouldAutoImportDiscoveredLogin(true, null)).toBe(false);
    expect(shouldAutoImportDiscoveredLogin(false, 'account')).toBe(false);
    expect(shouldAutoImportDiscoveredLogin(undefined, 'account')).toBe(true);
  });

  it('plans remaining agents only while auto-import is on', () => {
    expect(planLocalLoginAutoImport({
      autoImportLocalLogin: true,
      agentIds: ['claude', 'codex', 'kimi'],
      alreadyTried: new Set(['codex']),
    })).toEqual(['claude', 'kimi']);
    expect(planLocalLoginAutoImport({
      autoImportLocalLogin: false,
      agentIds: ['claude'],
      alreadyTried: new Set(),
    })).toEqual([]);
  });

  it('imports a discovered official login that is not already in the list', () => {
    expect(canAutoImportProbe({
      agentId: 'claude',
      poolState: 'ready',
      probe: { agentId: 'claude', kind: 'oauth', hasCredentials: true },
      accounts: [],
      providers: [],
    })).toBe(true);
  });

  it('does not auto-import when the same official login is already listed', () => {
    expect(canAutoImportProbe({
      agentId: 'claude',
      poolState: 'ready',
      probe: { agentId: 'claude', kind: 'oauth', hasCredentials: true },
      accounts: [{ kind: 'oauth' }],
      providers: [],
    })).toBe(false);
  });

  it('does not auto-import leftover local-route projections', () => {
    expect(canAutoImportProbe({
      agentId: 'claude',
      poolState: 'ready',
      probe: {
        agentId: 'claude',
        kind: 'api_key',
        hasCredentials: true,
        isAdapterProjection: true,
      },
      accounts: [],
      providers: [],
    })).toBe(false);
  });
});

describe('ticket add menu import visibility', () => {
  it('keeps 导入本机登录 when auto-import is off', () => {
    expect(ticketAddActionsForAgent(true, true, true).map((item) => item.kind)).toEqual([
      'import-login',
      'oauth',
      'api-key',
    ]);
  });

  it('hides 导入本机登录 when auto-import is on', () => {
    expect(ticketAddActionsForAgent(true, true, false).map((item) => item.kind)).toEqual([
      'oauth',
      'api-key',
    ]);
    expect(
      buildTicketAddMenu(['claude', 'cursor'], ['claude'], false).map((item) => ({
        id: item.id,
        kinds: item.actions.map((action) => action.kind),
      })),
    ).toEqual([
      { id: 'claude', kinds: ['oauth', 'api-key'] },
    ]);
  });
});
