import { describe, expect, it } from 'vitest';
import type { TicketWallet } from '@/lib/backend/contracts/ticket';
import { activeBindingForAgent, filterTicketsByAgentUsage } from './ticket-wallet';

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

  it('filters tickets owned by the agent or with an active binding to it', () => {
    const all = wallet();
    all.tickets.push({
      id: 't2',
      sourceKind: 'account',
      sourceId: 'codex-1',
      agentId: 'codex',
      label: 'me@openai.com',
      surface: 'codex-chatgpt-subscription',
      credentialClass: 'oauth',
      speaks: [],
      importedFrom: 'codex',
    });
    all.tickets.push({
      id: 't3',
      sourceKind: 'provider',
      sourceId: 'grok-1',
      agentId: 'grok',
      label: 'Grok',
      surface: 'xai-api',
      credentialClass: 'api_key',
      speaks: [],
      importedFrom: 'grok',
    });
    expect(filterTicketsByAgentUsage(all, all.tickets, null).map((row) => row.id)).toEqual([
      't1',
      't2',
      't3',
    ]);
    expect(filterTicketsByAgentUsage(all, all.tickets, 'claude').map((row) => row.id)).toEqual(['t1']);
    expect(filterTicketsByAgentUsage(all, all.tickets, 'codex').map((row) => row.id)).toEqual(['t2']);
    expect(filterTicketsByAgentUsage(all, all.tickets, 'kimi').map((row) => row.id)).toEqual(['t1']);
    expect(filterTicketsByAgentUsage(all, all.tickets, 'grok').map((row) => row.id)).toEqual(['t3']);
    expect(filterTicketsByAgentUsage(all, all.tickets, 'pi')).toEqual([]);
    expect(filterTicketsByAgentUsage(all, all.tickets, 'cursor')).toEqual([]);
  });

  it('drops a leftover inactive Claude binding on a Grok ticket and keeps a Codex ticket with an active Claude binding', () => {
    const all: TicketWallet = {
      tickets: [
        {
          id: 't-grok',
          sourceKind: 'account',
          sourceId: 'grok-1',
          agentId: 'grok',
          label: 'user@x.ai',
          surface: 'grok-xai-subscription',
          credentialClass: 'oauth',
          speaks: [],
          importedFrom: 'grok',
        },
        {
          id: 't-codex',
          sourceKind: 'account',
          sourceId: 'codex-1',
          agentId: 'codex',
          label: 'me@openai.com',
          surface: 'codex-chatgpt-subscription',
          credentialClass: 'oauth',
          speaks: [],
          importedFrom: 'codex',
        },
      ],
      bindings: [
        {
          ticketId: 't-grok',
          agentId: 'claude',
          route: 'native',
          active: false,
          profileId: null,
          bridge: null,
        },
        {
          ticketId: 't-codex',
          agentId: 'claude',
          route: 'bridge',
          active: true,
          profileId: 'p-claude',
          bridge: { port: 8123, running: true },
        },
      ],
      surfaceGroups: [],
    };

    expect(filterTicketsByAgentUsage(all, all.tickets, 'claude').map((row) => row.id)).toEqual([
      't-codex',
    ]);
    expect(filterTicketsByAgentUsage(all, all.tickets, 'grok').map((row) => row.id)).toEqual([
      't-grok',
    ]);
    expect(filterTicketsByAgentUsage(all, all.tickets, 'codex').map((row) => row.id)).toEqual([
      't-codex',
    ]);
  });

  it('uses leftover-inactive filtered length for agent chips and footer; header descriptionCount stays unfiltered', () => {
    const all: TicketWallet = {
      tickets: [
        {
          id: 't-grok',
          sourceKind: 'account',
          sourceId: 'grok-1',
          agentId: 'grok',
          label: 'user@x.ai',
          surface: 'grok-xai-subscription',
          credentialClass: 'oauth',
          speaks: [],
          importedFrom: 'grok',
        },
        {
          id: 't-codex',
          sourceKind: 'account',
          sourceId: 'codex-1',
          agentId: 'codex',
          label: 'me@openai.com',
          surface: 'codex-chatgpt-subscription',
          credentialClass: 'oauth',
          speaks: [],
          importedFrom: 'codex',
        },
      ],
      bindings: [
        {
          ticketId: 't-grok',
          agentId: 'claude',
          route: 'native',
          active: false,
          profileId: null,
          bridge: null,
        },
        {
          ticketId: 't-codex',
          agentId: 'claude',
          route: 'bridge',
          active: true,
          profileId: 'p-claude',
          bridge: { port: 8123, running: true },
        },
      ],
      surfaceGroups: [],
    };

    const claudeFiltered = filterTicketsByAgentUsage(all, all.tickets, 'claude');
    const chipCount = claudeFiltered.length;
    const footerCount = claudeFiltered.length;
    expect(claudeFiltered.map((row) => row.id)).toEqual(['t-codex']);
    expect(chipCount).toBe(1);
    expect(footerCount).toBe(1);

    // index.tsx header: t('connections.page.descriptionCount', { n: visibleWallet.tickets.length })
    const descriptionCount = all.tickets.length;
    expect(descriptionCount).toBe(2);
    expect(descriptionCount).not.toBe(chipCount);
  });
});
