import { describe, expect, it } from 'vitest';
import type { TicketWallet } from '@/lib/backend/contracts/ticket';
import { activeBindingForAgent } from './ticket-wallet';

function wallet(): TicketWallet {
  return {
    tickets: [
      {
        id: 't1',
        sourceKind: 'provider',
        sourceId: 'kimi-1',
        agentId: 'kimi',
        label: 'Kimi',
        surface: 'kimi-code-membership',
        credentialClass: 'api_key',
        speaks: [],
        importedFrom: 'kimi',
      },
    ],
    bindings: [
      {
        ticketId: 't1',
        agentId: 'claude',
        route: 'reshape',
        active: true,
        profileId: 'p1',
        bridge: null,
      },
      {
        ticketId: 't1',
        agentId: 'pi',
        route: 'native',
        active: false,
        profileId: 'p2',
        bridge: null,
      },
    ],
    surfaceGroups: [],
  };
}

describe('ticket-wallet', () => {
  it('returns the active TicketBinding for an agent', () => {
    const found = activeBindingForAgent(wallet(), 'claude');
    expect(found?.ticket.id).toBe('t1');
    expect(found?.binding.profileId).toBe('p1');
  });

  it('returns null when the agent has no active binding or the ticket is missing', () => {
    expect(activeBindingForAgent(wallet(), 'pi')).toBeNull();
    expect(activeBindingForAgent(wallet(), 'grok')).toBeNull();
    const orphan = wallet();
    orphan.tickets = [];
    expect(activeBindingForAgent(orphan, 'claude')).toBeNull();
  });

  it('returns the first active binding when duplicates exist', () => {
    const dup = wallet();
    dup.bindings.push({
      ticketId: 't1',
      agentId: 'claude',
      route: 'native',
      active: true,
      profileId: 'p-later',
      bridge: null,
    });
    expect(activeBindingForAgent(dup, 'claude')?.binding.profileId).toBe('p1');
  });
});
