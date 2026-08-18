import { describe, expect, it, vi } from 'vitest';
import { agentDisplayName } from '@/config/agents';
import type { Account, Provider } from '@/lib/types';
import type { TicketView, TicketWallet } from '@/lib/backend/contracts/ticket';
import {
  activeBindingForAgent,
  buildTicketAddMenu,
  dispatchTicketAddAction,
  ticketAddDialogState,
  buildTicketDetailFields,
  buildTicketWalletRows,
  countTicketsByFilter,
  dashboardBindingMetaText,
  extrasFromPoolSource,
  filterTickets,
  findTicketPoolSource,
  formatTicketBindingDetailLines,
  formatTicketUsageParts,
  formatTicketUsageText,
  isUnrecognizedTicket,
  searchTickets,
  ticketDetailEditLabel,
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

  it('matches「正用于」agent and route labels (Codex / 本机路由)', () => {
    const wallet = sampleWallet();
    expect(searchTickets(wallet.tickets, 'Codex', wallet.bindings).map((t) => t.id))
      .toEqual(['provider:kimi-1']);
    expect(searchTickets(wallet.tickets, '本机路由', wallet.bindings).map((t) => t.id))
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
    expect(formatTicketUsageText(kimiBindings, 'kimi')).toContain('正用于：');
    expect(formatTicketUsageText(kimiBindings, 'kimi')).toContain('改配置');
    expect(formatTicketUsageText(kimiBindings, 'kimi')).toContain('本机路由 · 运行中');
    expect(formatTicketUsageText([])).toBe('未使用');
    expect(formatTicketUsageText([], 'codex')).toBe(`${agentDisplayName('codex')} · 未使用`);
    const parts = formatTicketUsageParts(kimiBindings, 'kimi');
    expect(parts.some((part) => part.kind === 'bridge' && part.href === '/routes?profile=p2')).toBe(true);
    expect(formatTicketUsageParts([{
      ticketId: 'provider:kimi-1',
      agentId: 'codex',
      route: 'bridge',
      active: true,
      profileId: null,
      bridge: { port: 8123, running: true },
    }]).some((part) => part.kind === 'bridge' && part.href === '/routes')).toBe(true);
  });

  it('keeps self-use on one phrase so the row does not repeat the owner', () => {
    expect(formatTicketUsageText([{
      ticketId: 'account:codex-1',
      agentId: 'codex',
      route: 'native',
      active: true,
      profileId: null,
      bridge: null,
    }], 'codex')).toBe(`${agentDisplayName('codex')}（切换）`);
    expect(formatTicketUsageText([{
      ticketId: 'account:codex-1',
      agentId: 'codex',
      route: 'native',
      active: true,
      profileId: null,
      bridge: null,
    }], 'codex')).not.toContain('正用于：');
  });

  it('maps dashboard meta text', () => {
    expect(dashboardBindingMetaText('Kimi 会员', 'reshape')).toBe('Kimi 会员 · 改配置');
    expect(dashboardBindingMetaText('Kimi 会员', 'bridge')).toBe('Kimi 会员 · 本机路由');
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

function ticket(partial: Partial<TicketView> & Pick<TicketView, 'id'>): TicketView {
  return {
    sourceKind: 'provider',
    sourceId: 'kimi-1',
    agentId: 'kimi',
    label: 'Kimi 会员',
    surface: 'kimi-code-membership',
    credentialClass: 'api_key',
    speaks: ['anthropic-messages'],
    importedFrom: 'kimi',
    ...partial,
  };
}

function account(partial: Partial<Account> & Pick<Account, 'id' | 'kind' | 'label'>): Account {
  return {
    agentId: 'claude',
    isCurrent: false,
    tokenValid: true,
    ...partial,
  };
}

function provider(partial: Partial<Provider> & Pick<Provider, 'id' | 'name'>): Provider {
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

describe('ticket detail fields', () => {
  it('shows ticket identity without opening an edit form', () => {
    const fields = buildTicketDetailFields(ticket({ id: 'provider:kimi-1' }), {
      endpointMode: 'custom',
      endpointHost: 'relay.example.com',
    });
    expect(fields).toEqual(expect.arrayContaining([
      { label: '类型', value: 'API Key' },
      { label: '来源', value: '会员' },
      { label: '所属', value: agentDisplayName('kimi') },
      { label: '导入自', value: agentDisplayName('kimi') },
      { label: '端点', value: '自定义' },
      { label: 'Endpoint', value: 'relay.example.com', mono: true },
      { label: '协议', value: 'anthropic-messages' },
    ]));
  });

  it('lists bindings including inactive and stopped bridges', () => {
    const wallet = sampleWallet();
    expect(formatTicketBindingDetailLines(
      wallet.bindings.filter((binding) => binding.ticketId === 'provider:kimi-1'),
    )).toEqual([
      `${agentDisplayName('claude')} · 改配置 · 当前`,
      `${agentDisplayName('codex')} · 本机路由 · 当前 · 运行中 · 端口 8123`,
    ]);
    expect(formatTicketBindingDetailLines(
      wallet.bindings.filter((binding) => binding.ticketId === 'account:oauth-1'),
    )).toEqual([`${agentDisplayName('claude')} · 切换`]);
  });

  it('joins pool extras so OAuth can be inspected and API Key can be edited', () => {
    const oauth = ticket({
      id: 'account:oauth-1',
      sourceKind: 'account',
      sourceId: 'oauth-1',
      agentId: 'claude',
      label: 'me@example.com',
      surface: 'claude-subscription',
      credentialClass: 'oauth',
      speaks: [],
      importedFrom: 'claude',
    });
    const source = findTicketPoolSource(oauth, [
      account({
        id: 'oauth-1',
        kind: 'oauth',
        label: 'me@example.com',
        email: 'me@example.com',
        subscription: 'Pro',
        quota5hPct: 40,
      }),
    ], []);
    const extras = extrasFromPoolSource(oauth, source);
    expect(extras.identity).toBe('me@example.com');
    expect(extras.canEditKey).toBe(false);
    expect(extras.canEditConfig).toBe(false);
    expect(ticketDetailEditLabel(extras)).toBeNull();

    const keyTicket = ticket({ id: 'provider:kimi-1' });
    const keyExtras = extrasFromPoolSource(
      keyTicket,
      findTicketPoolSource(keyTicket, [], [
        provider({ id: 'kimi-1', agentId: 'kimi', name: 'Kimi 会员' }),
      ]),
    );
    expect(keyExtras.canEditConfig).toBe(true);
    expect(keyExtras.endpointHost).toBe('relay.example.com');
    expect(ticketDetailEditLabel(keyExtras)).toBe('编辑配置');
  });
});

describe('buildTicketAddMenu', () => {
  it('nests import and API Key under each Agent', () => {
    const menu = buildTicketAddMenu(['claude', 'kimi']);
    expect(menu.map((item) => item.id)).toEqual(['claude', 'kimi']);
    expect(menu[0]?.name).toBe(agentDisplayName('claude'));
    expect(menu.map((item) => item.actions.map((a) => a.kind))).toEqual([
      ['import-login', 'api-key'],
      ['import-login', 'api-key'],
    ]);
    expect(menu[0]?.actions.map((a) => a.label)).toEqual(['导入当前登录', '添加 API Key']);
  });

  it('is empty when no Agent is installed', () => {
    expect(buildTicketAddMenu([])).toEqual([]);
    expect(buildTicketAddMenu(null)).toEqual([]);
    expect(buildTicketAddMenu()).toEqual([]);
  });
});

describe('dispatchTicketAddAction', () => {
  it('forwards the selected Agent to the matching handler', () => {
    const onImportLogin = vi.fn();
    const onAddKey = vi.fn();
    dispatchTicketAddAction('import-login', 'kimi', { onImportLogin, onAddKey });
    expect(onImportLogin).toHaveBeenCalledOnce();
    expect(onImportLogin).toHaveBeenCalledWith('kimi');
    expect(onAddKey).not.toHaveBeenCalled();

    onImportLogin.mockClear();
    dispatchTicketAddAction('api-key', 'claude', { onImportLogin, onAddKey });
    expect(onAddKey).toHaveBeenCalledOnce();
    expect(onAddKey).toHaveBeenCalledWith('claude');
    expect(onImportLogin).not.toHaveBeenCalled();
  });

  it('no-ops when the matching handler is missing', () => {
    expect(() => dispatchTicketAddAction('import-login', 'kimi', {})).not.toThrow();
    expect(() => dispatchTicketAddAction('api-key', 'claude', {})).not.toThrow();
  });
});

describe('ticketAddDialogState', () => {
  it('opens import or API Key against the submenu Agent', () => {
    expect(ticketAddDialogState('import-login', 'codex')).toEqual({
      addAgentId: 'codex',
      loginImportOpen: true,
      apiKeyDialogOpen: false,
      clearEditProvider: false,
    });
    expect(ticketAddDialogState('api-key', 'grok')).toEqual({
      addAgentId: 'grok',
      loginImportOpen: false,
      apiKeyDialogOpen: true,
      clearEditProvider: true,
    });
  });
});
