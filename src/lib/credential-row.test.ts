import { describe, expect, it } from 'vitest';
import type { Account, Provider } from '@/lib/types';
import type { TicketView } from '@/lib/backend/contracts/ticket';
import { toCredentialRow } from './credential-row';

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

function ticket(partial: Partial<TicketView> & Pick<TicketView, 'id' | 'sourceKind' | 'sourceId' | 'label'>): TicketView {
  return {
    agentId: 'claude',
    surface: 'anthropic-api',
    credentialClass: 'api_key',
    speaks: [],
    importedFrom: null,
    ...partial,
  };
}

describe('toCredentialRow', () => {
  it('projects account stable fields and auth summary', () => {
    const row = toCredentialRow({
      source: 'account',
      account: acc({ id: 'a1', kind: 'oauth', label: 'me@x.com', isCurrent: true, subscription: 'Pro' }),
    });
    expect(row).toMatchObject({
      key: 'account:a1',
      source: 'account',
      id: 'a1',
      agentId: 'claude',
      title: 'me@x.com',
      isCurrent: true,
    });
    expect(row.subtitle).toContain('Pro');
    expect(row.auth.label).toBeTruthy();
    expect(row.auth.status).toBeTruthy();
  });

  it('projects provider stable fields with endpoint subtitle', () => {
    const row = toCredentialRow({
      source: 'provider',
      provider: prov({ id: 'p1', name: 'Relay' }),
    });
    expect(row).toMatchObject({
      key: 'provider:p1',
      source: 'provider',
      id: 'p1',
      title: 'Relay',
      isCurrent: false,
    });
    expect(row.subtitle).toContain('自定义端点');
    expect(row.auth.health).toBe('configured');
  });

  it('projects ticket along the same axis', () => {
    const row = toCredentialRow({
      source: 'ticket',
      ticket: ticket({
        id: 'account:t1',
        sourceKind: 'account',
        sourceId: 't1',
        label: 'Claude OAuth',
        credentialClass: 'oauth',
        surface: 'claude-subscription',
      }),
      isCurrent: true,
    });
    expect(row).toMatchObject({
      key: 'account:t1',
      source: 'account',
      id: 't1',
      title: 'Claude OAuth',
      isCurrent: true,
    });
    expect(row.subtitle).toContain('官方登录');
  });
});
