import { describe, expect, it } from 'vitest';
import type { TicketWallet } from '@/lib/backend/contracts/ticket';
import {
  activeBindingForAgent,
  buildTicketWalletRows,
  countTicketsByFilter,
  dashboardBindingMetaText,
  filterTickets,
  formatTicketUsageText,
  isUnrecognizedTicket,
  searchTickets,
} from './ticket-wallet-model';

function sampleWallet(): TicketWallet {
  return {
    tickets: [
      {
        id: 'provider:kimi-1',
        sourceKind: 'provider',
        sourceId: 'kimi-1',
        agentId: 'kimi',
        label: 'Kimi 会员',
        surface: 'kimi-code-membership',
        credentialClass: 'api_key',
        speaks: ['anthropic-messages', 'openai-chat'],
        importedFrom: 'kimi',
      },
      {
        id: 'provider:ant-1',
        sourceKind: 'provider',
        sourceId: 'ant-1',
        agentId: 'claude',
        label: 'Anthropic Key',
        surface: 'anthropic-api',
        credentialClass: 'api_key',
        speaks: ['anthropic-messages'],
        importedFrom: 'claude',
      },
      {
        id: 'provider:unk-1',
        sourceKind: 'provider',
        sourceId: 'unk-1',
        agentId: 'claude',
        label: '自定义中转',
        // Production shape: unknown surface keeps real credential class.
        surface: 'unknown',
        credentialClass: 'api_key',
        speaks: [],
        importedFrom: 'claude',
      },
      {
        id: 'account:oauth-1',
        sourceKind: 'account',
        sourceId: 'oauth-1',
        agentId: 'claude',
        label: 'me@example.com',
        surface: 'unknown',
        credentialClass: 'oauth',
        speaks: [],
        importedFrom: 'claude',
      },
    ],
    bindings: [
      {
        ticketId: 'provider:kimi-1',
        agentId: 'claude',
        route: 'reshape',
        active: true,
        profileId: 'p1',
        bridge: null,
      },
      {
        ticketId: 'provider:kimi-1',
        agentId: 'codex',
        route: 'bridge',
        active: true,
        profileId: 'p2',
        bridge: { port: 8123, running: true },
      },
      {
        ticketId: 'account:oauth-1',
        agentId: 'claude',
        route: 'native',
        active: false,
        profileId: null,
        bridge: null,
      },
    ],
  };
}

describe('ticket wallet filter / search', () => {
  it('counts and filters 未识别 by surface (production unknown + api_key shape)', () => {
    const tickets = sampleWallet().tickets;
    expect(isUnrecognizedTicket(tickets[2]!)).toBe(true);
    expect(countTicketsByFilter(tickets)).toEqual({
      all: 4,
      oauth: 1,
      api_key: 3,
      unknown: 2,
    });
    expect(filterTickets(tickets, 'oauth').map((t) => t.id)).toEqual(['account:oauth-1']);
    expect(filterTickets(tickets, 'unknown').map((t) => t.id)).toEqual([
      'provider:unk-1',
      'account:oauth-1',
    ]);
  });

  it('searches by label and surface synonyms', () => {
    const tickets = sampleWallet().tickets;
    expect(searchTickets(tickets, '会员').map((t) => t.id)).toEqual(['provider:kimi-1']);
    expect(searchTickets(tickets, '官方登录').map((t) => t.id)).toEqual(['account:oauth-1']);
    expect(searchTickets(tickets, '未识别').map((t) => t.id)).toEqual([
      'provider:unk-1',
      'account:oauth-1',
    ]);
  });

  it('matches「正用于」agent and route labels (Codex / 本机桥)', () => {
    const wallet = sampleWallet();
    expect(searchTickets(wallet.tickets, 'Codex', wallet.bindings).map((t) => t.id))
      .toEqual(['provider:kimi-1']);
    expect(searchTickets(wallet.tickets, '本机桥', wallet.bindings).map((t) => t.id))
      .toEqual(['provider:kimi-1']);
    expect(searchTickets(wallet.tickets, '改配置', wallet.bindings).map((t) => t.id))
      .toEqual(['provider:kimi-1']);
    expect(buildTicketWalletRows(wallet, { query: 'Codex' }).map((r) => r.ticket.id))
      .toEqual(['provider:kimi-1']);
  });
});

describe('binding usage text', () => {
  it('formats active bindings with route labels', () => {
    const wallet = sampleWallet();
    const kimiBindings = wallet.bindings.filter((b) => b.ticketId === 'provider:kimi-1');
    expect(formatTicketUsageText(kimiBindings)).toContain('正用于：');
    expect(formatTicketUsageText(kimiBindings)).toContain('改配置');
    expect(formatTicketUsageText(kimiBindings)).toContain('本机桥 · 运行中');
    expect(formatTicketUsageText([])).toBe('未使用');
  });

  it('maps dashboard meta text', () => {
    expect(dashboardBindingMetaText('Kimi 会员', 'reshape')).toBe('Kimi 会员 · 改配置');
    expect(dashboardBindingMetaText('Kimi 会员', 'bridge')).toBe('Kimi 会员 · 本机桥');
    expect(dashboardBindingMetaText('me@…', 'native')).toBe('me@… · 直连');
  });
});

describe('buildTicketWalletRows', () => {
  it('highlights deep-link agent active bindings without privatizing the list', () => {
    const wallet = sampleWallet();
    const rows = buildTicketWalletRows(wallet, { highlightAgentId: 'claude' });
    expect(rows).toHaveLength(4);
    const kimi = rows.find((r) => r.ticket.id === 'provider:kimi-1');
    const oauth = rows.find((r) => r.ticket.id === 'account:oauth-1');
    expect(kimi?.highlighted).toBe(true);
    expect(oauth?.highlighted).toBe(false);
  });

  it('finds active binding for dashboard agent', () => {
    const wallet = sampleWallet();
    const hit = activeBindingForAgent(wallet, 'codex');
    expect(hit?.ticket.label).toBe('Kimi 会员');
    expect(hit?.binding.route).toBe('bridge');
    expect(activeBindingForAgent(wallet, 'pi')).toBeNull();
  });
});
