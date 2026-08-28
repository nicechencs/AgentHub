import { describe, expect, it, vi } from 'vitest';
import { createTranslator } from '@/lib/i18n';
import { agentDisplayName } from '@/config/agents';
import type { Account, Provider } from '@/lib/types';
import type { BindingView, TicketView, TicketWallet } from '@/lib/backend/contracts/ticket';
import {
  activeBindingForAgent,
  buildTicketAddMenu,
  focusedTicketAddAgent,
  dispatchTicketAddAction,
  armMenuDialogOpen,
  handleMenuDialogSelect,
  handleTicketAddMenuSelect,
  ticketAddMenuClosesOnKey,
  MENU_DIALOG_DISMISS_CLEAR_MS,
  shouldIgnoreMenuDialogDismiss,
  ticketAddDialogState,
  buildTicketBindingRows,
  buildTicketDetailFields,
  buildTicketWalletRows,
  countTicketsByFilter,
  filterWalletByExcludedAgents,
  dashboardBindingMetaText,
  extrasFromPoolSource,
  filterTickets,
  hasOfficialQuotaWindow,
  findTicketPoolSource,
  formatTicketBindingDetailLines,
  formatTicketUsageParts,
  formatTicketUsageText,
  humanizeTicketAuthLabel,
  ticketAuthChip,
  ticketCardTitle,
  ticketSwitchChip,
  showsNativeSwitch,
  isUnrecognizedTicket,
  ticketBindingStatus,
  ticketDetailEditLabel,
  ticketWalletFilterLabel,
  ticketCredentialClassChipLabel,
  ticketSurfaceChipLabel,
  resolveTicketRouteAction,
  resolveTicketShareAction,
  ticketSwitchDisabledReason,
  ticketRefreshDisabledReason,
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
    surfaceGroups: [],
  };
}

describe('filterWalletByExcludedAgents', () => {
  it('drops tickets and bindings for omitted (hidden or uninstalled) agents', () => {
    const wallet = sampleWallet();
    const next = filterWalletByExcludedAgents(wallet, ['claude']);
    expect(next?.tickets.map((ticket) => ticket.id)).toEqual(['provider:kimi-1']);
    expect(next?.bindings.map((binding) => `${binding.ticketId}:${binding.agentId}`)).toEqual([
      'provider:kimi-1:codex',
    ]);
  });

  it('keeps an omitted-owner ticket that is still bound to a visible agent', () => {
    const wallet = sampleWallet();
    const next = filterWalletByExcludedAgents(wallet, ['kimi']);
    expect(next?.tickets.map((ticket) => ticket.id)).toContain('provider:kimi-1');
    expect(
      next?.bindings.map((binding) => `${binding.ticketId}:${binding.agentId}`),
    ).toEqual(expect.arrayContaining(['provider:kimi-1:claude', 'provider:kimi-1:codex']));
  });

  it('returns the same wallet when the exclude set is empty', () => {
    const wallet = sampleWallet();
    expect(filterWalletByExcludedAgents(wallet, [])).toBe(wallet);
    expect(filterWalletByExcludedAgents(null, ['kimi'])).toBeNull();
  });
});

describe('ticket wallet filter', () => {
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
});

describe('hasOfficialQuotaWindow', () => {
  it('hides missing official percents and shows 0 as a real value', () => {
    expect(hasOfficialQuotaWindow(undefined)).toBe(false);
    expect(hasOfficialQuotaWindow(null)).toBe(false);
    expect(hasOfficialQuotaWindow(Number.NaN)).toBe(false);
    expect(hasOfficialQuotaWindow(0)).toBe(true);
    expect(hasOfficialQuotaWindow(40)).toBe(true);
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

  it('annotates bridge usage with N-member poll pool copy', () => {
    const wallet = sampleWallet();
    wallet.surfaceGroups = [{
      surface: 'kimi-code-membership',
      credentialClass: 'api_key',
      members: [
        {
          ticketId: 'account:kimi-stale',
          sourceKind: 'account',
          sourceId: 'kimi-stale',
          agentId: 'kimi',
          label: 'Kimi 会员（失效号）',
          health: 'needs_login',
        },
        {
          ticketId: 'provider:kimi-1',
          sourceKind: 'provider',
          sourceId: 'kimi-1',
          agentId: 'kimi',
          label: 'Kimi 会员',
          health: 'renewable',
        },
      ],
    }];
    const rows = buildTicketWalletRows(wallet);
    const kimi = rows.find((row) => row.ticket.id === 'provider:kimi-1');
    expect(kimi?.usageText).toContain('2 份同类登录可轮换');
    expect(kimi?.usageText).toContain('本机路由');
    expect(kimi?.usageText).toContain('运行中');
    const ant = rows.find((row) => row.ticket.id === 'provider:ant-1');
    expect(ant?.usageText).not.toContain('可轮换');
  });

  it('keeps self-use on one phrase so the row does not repeat the owner', () => {
    expect(formatTicketUsageText([{
      ticketId: 'account:codex-1',
      agentId: 'codex',
      route: 'native',
      active: true,
      profileId: null,
      bridge: null,
    }], 'codex')).toBe(`${agentDisplayName('codex')}（直连）`);
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

  it('filters cards to the selected agent', () => {
    const wallet = sampleWallet();
    const claude = buildTicketWalletRows(wallet, { agentFilterId: 'claude' });
    const grok = buildTicketWalletRows(wallet, { agentFilterId: 'grok' });
    const kimi = buildTicketWalletRows(wallet, { agentFilterId: 'kimi' });
    expect(claude.map((row) => row.ticket.id).sort()).toEqual([
      'account:oauth-1',
      'provider:ant-1',
      'provider:unk-1',
    ].sort());
    expect(claude.some((row) => row.ticket.agentId === 'kimi')).toBe(false);
    expect(kimi.map((row) => row.ticket.id)).toEqual(['provider:kimi-1']);
    expect(grok).toEqual([]);
    expect(claude.length + kimi.length + grok.length).toBe(wallet.tickets.length);
  });

  it('does not keep a Grok ticket for a leftover inactive Claude binding; keeps a Codex ticket with an active Claude binding', () => {
    const wallet: TicketWallet = {
      tickets: [
        {
          id: 'account:grok-1',
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
          id: 'account:codex-1',
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
          ticketId: 'account:grok-1',
          agentId: 'claude',
          route: 'native',
          active: false,
          profileId: null,
          bridge: null,
        },
        {
          ticketId: 'account:codex-1',
          agentId: 'claude',
          route: 'bridge',
          active: true,
          profileId: 'p-claude',
          bridge: { port: 8123, running: true },
        },
      ],
      surfaceGroups: [],
    };

    const claude = buildTicketWalletRows(wallet, { agentFilterId: 'claude' });
    expect(claude.map((row) => row.ticket.id)).toEqual([]);
    expect(claude.some((row) => row.ticket.agentId === 'grok')).toBe(false);

    const grok = buildTicketWalletRows(wallet, { agentFilterId: 'grok' });
    expect(grok.map((row) => row.ticket.id)).toEqual(['account:grok-1']);

    const codex = buildTicketWalletRows(wallet, { agentFilterId: 'codex' });
    expect(codex.map((row) => row.ticket.id)).toEqual(['account:codex-1']);
  });

  it('uses leftover-inactive filtered length for chips and footer; header descriptionCount stays unfiltered', () => {
    const wallet: TicketWallet = {
      tickets: [
        {
          id: 'account:grok-1',
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
          id: 'account:codex-1',
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
          ticketId: 'account:grok-1',
          agentId: 'claude',
          route: 'native',
          active: false,
          profileId: null,
          bridge: null,
        },
        {
          ticketId: 'account:codex-1',
          agentId: 'claude',
          route: 'bridge',
          active: true,
          profileId: 'p-claude',
          bridge: { port: 8123, running: true },
        },
      ],
      surfaceGroups: [],
    };

    const t = createTranslator('zh');
    const claudeRows = buildTicketWalletRows(wallet, { agentFilterId: 'claude' });
    const grokRows = buildTicketWalletRows(wallet, { agentFilterId: 'grok' });
    const codexRows = buildTicketWalletRows(wallet, { agentFilterId: 'codex' });
    const chipCount = claudeRows.length + grokRows.length + codexRows.length;
    expect(claudeRows).toHaveLength(0);
    expect(grokRows).toHaveLength(1);
    expect(codexRows).toHaveLength(1);
    expect(chipCount).toBe(wallet.tickets.length);
    expect(t('connections.list.count', { n: grokRows.length })).toBe('1 份登录');

    const descriptionCount = wallet.tickets.length;
    expect(descriptionCount).toBe(2);
    expect(t('connections.page.descriptionCount', { n: descriptionCount })).toBe('2 份登录');
  });

  it('finds active binding for dashboard agent', () => {
    const wallet = sampleWallet();
    const hit = activeBindingForAgent(wallet, 'codex');
    expect(hit?.ticket.label).toBe('Kimi 会员');
    expect(hit?.binding.route).toBe('bridge');
    expect(activeBindingForAgent(wallet, 'pi')).toBeNull();
  });
});

function bridgeBinding(
  ticketId: string,
  agentId: BindingView['agentId'],
  profileId = 'p-or',
): BindingView {
  return {
    ticketId,
    agentId,
    route: 'bridge',
    active: true,
    profileId,
    bridge: { port: 43121, running: true },
  };
}

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
  it('keeps API Key custom endpoint facts under advanced only', () => {
    const { advanced } = buildTicketDetailFields(ticket({ id: 'provider:kimi-1' }), {
      endpointMode: 'custom',
      endpointHost: 'https://relay.example.com/v1',
    });
    expect(advanced).toEqual(expect.arrayContaining([
      { label: '端点', value: '自定义' },
      { label: '主机', value: 'relay.example.com', mono: true },
      { label: '接口', value: 'Claude' },
    ]));
    const customLabels = advanced.map((field) => field.label);
    expect(customLabels).not.toContain('导入自');
    expect(customLabels).not.toContain('登录状态');
    expect(customLabels).not.toEqual(
      expect.arrayContaining(['类型', '来源', '所属', '官方账号', '提供商']),
    );
  });

  it('omits import, login status, and protocol for official OAuth', () => {
    const { advanced, protocol } = buildTicketDetailFields(
      ticket({
        id: 'account:oauth-1',
        sourceKind: 'account',
        sourceId: 'oauth-1',
        agentId: 'grok',
        label: 'me@example.com',
        surface: 'grok-xai-subscription',
        credentialClass: 'oauth',
        speaks: ['openai-chat', 'xai-device-code'],
        importedFrom: 'grok',
      }),
      {
        identity: 'me@example.com',
        accountProvider: 'https://accounts.x.ai/oauth',
        authLabel: '可续期·未验证',
        endpointMode: 'official',
        endpointHost: 'accounts.x.ai',
      },
    );
    const labels = advanced.map((field) => field.label);
    expect(advanced).toEqual([]);
    expect(protocol).toBe('Chat · Grok');
    expect(labels).not.toContain('导入自');
    expect(labels).not.toContain('登录状态');
    expect(labels).not.toContain('类型');
    expect(labels).not.toContain('来源');
    expect(labels).not.toContain('所属');
    expect(labels).not.toContain('官方账号');
    expect(labels).not.toContain('协议');
    expect(labels).not.toContain('提供商');
    expect(labels).not.toContain('端点');
    expect(labels).not.toContain('Endpoint');
  });

  it('classifies mytokens.cc custom remote as a recognized API Key, not 未识别', () => {
    const row = ticket({
      id: 'provider:codex-mytokens',
      sourceId: 'codex-mytokens',
      agentId: 'codex',
      label: 'OpenAI · gpt-5.5',
      surface: 'openai-api',
      credentialClass: 'api_key',
      speaks: ['openai-chat'],
    });
    expect(isUnrecognizedTicket(row)).toBe(false);
    expect(ticketSurfaceChipLabel(row.surface)).not.toBe('未识别');
    const { advanced } = buildTicketDetailFields(row, {
      endpointMode: 'custom',
      endpointHost: 'https://mytokens.cc/v1',
    });
    expect(advanced).toEqual(expect.arrayContaining([
      { label: '端点', value: '自定义' },
      { label: '主机', value: 'mytokens.cc', mono: true },
      { label: '接口', value: 'Chat' },
    ]));
    expect(advanced.map((field) => field.value).join(' ')).not.toContain('未识别');
  });

  it('shows 端点 / 主机 for custom API Key, and skips 接口 when speaks is empty', () => {
    const { advanced } = buildTicketDetailFields(
      ticket({
        id: 'provider:unk-1',
        sourceId: 'unk-1',
        agentId: 'claude',
        label: '自定义中转',
        surface: 'unknown',
        credentialClass: 'api_key',
        speaks: [],
      }),
      {
        endpointMode: 'custom',
        endpointHost: 'https://relay.example.com/v1',
      },
    );
    expect(advanced).toEqual([
      { label: '端点', value: '自定义' },
      { label: '主机', value: 'relay.example.com', mono: true },
    ]);
  });

  it('keeps OpenRouter interface as Chat and does not relabel it Claude', () => {
    const { advanced } = buildTicketDetailFields(
      ticket({
        id: 'provider:or-1',
        sourceId: 'or-1',
        agentId: 'claude',
        label: 'OpenRouter',
        surface: 'unknown',
        credentialClass: 'api_key',
        speaks: ['openai-chat'],
      }),
      {
        endpointMode: 'custom',
        endpointHost: 'https://openrouter.ai/api/v1',
      },
    );
    expect(advanced).toEqual(expect.arrayContaining([
      { label: '端点', value: '自定义' },
      { label: '主机', value: 'openrouter.ai', mono: true },
      { label: '接口', value: 'Chat' },
    ]));
    expect(advanced.map((field) => field.value).join(' ')).not.toContain('anthropic-messages');
  });

  it('shows 本机路由 for an OpenRouter URL when bound to Claude', () => {
    const { advanced } = buildTicketDetailFields(
      ticket({
        id: 'provider:or-1',
        sourceId: 'or-1',
        agentId: 'claude',
        label: 'OpenRouter',
        surface: 'unknown',
        credentialClass: 'api_key',
        speaks: ['openai-chat'],
      }),
      {
        endpointMode: 'custom',
        endpointHost: 'https://openrouter.ai/api/v1',
      },
      undefined,
      [bridgeBinding('provider:or-1', 'claude')],
    );
    expect(advanced).toEqual(expect.arrayContaining([
      { label: '接口', value: 'Chat' },
      { label: '主机', value: 'openrouter.ai', mono: true },
    ]));
    expect(advanced).toEqual(expect.arrayContaining([
      { label: '本机路由', value: 'Claude', mono: true },
    ]));
  });

  it('adds Codex / Grok / Kimi client names for loopback 本机路由', () => {
    expect(buildTicketDetailFields(
      ticket({ id: 'account:oauth-codex', sourceKind: 'account', sourceId: 'oauth-codex', agentId: 'codex', credentialClass: 'oauth', speaks: ['openai-responses'] }),
      { identity: 'me@example.com' },
      undefined,
      [bridgeBinding('account:oauth-codex', 'codex')],
    ).advanced).toEqual(expect.arrayContaining([
      { label: '本机路由', value: 'Codex', mono: true },
    ]));
    expect(buildTicketDetailFields(
      ticket({ id: 'account:oauth-grok', sourceKind: 'account', sourceId: 'oauth-grok', agentId: 'grok', credentialClass: 'oauth', speaks: ['openai-responses'] }),
      { identity: 'me@example.com' },
      undefined,
      [bridgeBinding('account:oauth-grok', 'grok')],
    ).advanced).toEqual(expect.arrayContaining([
      { label: '本机路由', value: 'Grok', mono: true },
    ]));
    expect(buildTicketDetailFields(
      ticket({ id: 'account:oauth-codex-kimi', sourceKind: 'account', sourceId: 'oauth-codex-kimi', agentId: 'codex', credentialClass: 'oauth', speaks: ['openai-responses'] }),
      { identity: 'me@example.com' },
      undefined,
      [bridgeBinding('account:oauth-codex-kimi', 'kimi')],
    ).advanced).toEqual(expect.arrayContaining([
      { label: '本机路由', value: 'Kimi', mono: true },
    ]));
  });

  it('joins distinct 本机路由 surfaces for official OAuth and OpenRouter URLs', () => {
    const { advanced: mixed } = buildTicketDetailFields(
      ticket({
        id: 'account:oauth-multi',
        sourceKind: 'account',
        sourceId: 'oauth-multi',
        agentId: 'codex',
        credentialClass: 'oauth',
        speaks: ['openai-responses'],
      }),
      { identity: 'me@example.com' },
      undefined,
      [
        bridgeBinding('account:oauth-multi', 'claude'),
        bridgeBinding('account:oauth-multi', 'codex', 'p-codex'),
      ],
    );
    expect(mixed).toEqual(expect.arrayContaining([
      { label: '本机路由', value: 'Claude · Codex', mono: true },
    ]));

    const { advanced: openrouter } = buildTicketDetailFields(
      ticket({
        id: 'provider:or-multi',
        sourceId: 'or-multi',
        agentId: 'claude',
        speaks: ['openai-chat'],
      }),
      { endpointMode: 'custom', endpointHost: 'https://openrouter.ai/api/v1' },
      undefined,
      [
        bridgeBinding('provider:or-multi', 'claude'),
        bridgeBinding('provider:or-multi', 'codex', 'p-or-codex'),
      ],
    );
    expect(openrouter).toEqual(expect.arrayContaining([
      { label: '本机路由', value: 'Claude · Codex', mono: true },
    ]));
  });

  it('does not add 本机路由 for reshape or native bindings', () => {
    const extras = {
      endpointMode: 'custom' as const,
      endpointHost: 'https://relay.example.com/v1',
    };
    const { advanced } = buildTicketDetailFields(
      ticket({ id: 'provider:kimi-reshape', sourceId: 'kimi-reshape' }),
      extras,
      undefined,
      [{
        ticketId: 'provider:kimi-reshape',
        agentId: 'claude',
        route: 'reshape',
        active: true,
        profileId: 'p-reshape',
        bridge: null,
      }, {
        ticketId: 'provider:kimi-reshape',
        agentId: 'kimi',
        route: 'native',
        active: false,
        profileId: null,
        bridge: null,
      }],
    );
    expect(advanced.map((field) => field.label)).not.toContain('本机路由');
  });

  it('humanizes login health without 未验证', () => {
    expect(humanizeTicketAuthLabel('可续期·未验证')).toBe('可续期');
    expect(humanizeTicketAuthLabel('已配置·未验证')).toBe('已配置');
    expect(humanizeTicketAuthLabel('可续期，尚未验证')).toBe('可续期');
    expect(humanizeTicketAuthLabel('已配置，尚未验证')).toBe('已配置');
    expect(humanizeTicketAuthLabel('可续期')).toBe('可续期');
    expect(humanizeTicketAuthLabel('已配置')).toBe('已配置');
    expect(humanizeTicketAuthLabel('已验证')).toBe('已验证');
  });

  it('replaces 可续期 / 已配置 chips with the secret tail', () => {
    expect(ticketAuthChip({
      authLabel: '可续期·未验证',
      secretTail: '**JF6Q',
    })).toEqual({ label: '**JF6Q', mono: true });
    expect(ticketAuthChip({
      authLabel: '已配置',
      secretTail: '**wxyz',
    })).toEqual({ label: '**wxyz', mono: true });
    expect(ticketAuthChip({ authLabel: '可续期·未验证' })).toEqual({
      label: '可续期',
      mono: false,
    });
    expect(ticketAuthChip({ authLabel: '已验证', secretTail: '**JF6Q' })).toEqual({
      label: '已验证',
      mono: false,
    });
  });

  it('prefers healed email over placeholder ticket labels', () => {
    expect(ticketCardTitle(
      { label: 'codex oauth' },
      { identity: 'user@example.com' },
    )).toBe('user@example.com');
    expect(ticketCardTitle(
      { label: 'codex-oauth' },
      { accountLabel: 'user@example.com' },
    )).toBe('user@example.com');
    expect(ticketCardTitle(
      { label: 'codex oauth' },
      { identity: '官方未提供账号信息', accountLabel: 'codex-oauth' },
    )).toBe('codex oauth');
  });

  it('hides native 切换 on a foreign Agent usage tab', () => {
    expect(showsNativeSwitch('kimi', null)).toBe(true);
    expect(showsNativeSwitch('kimi', 'kimi')).toBe(true);
    expect(showsNativeSwitch('kimi', 'codex')).toBe(false);
  });

  it('uses 切换 for idle grants and 使用中 when current', () => {
    expect(ticketSwitchChip()).toEqual({ kind: 'switch', label: '切换' });
    expect(ticketSwitchChip({ isCurrent: false })).toEqual({ kind: 'switch', label: '切换' });
    expect(ticketSwitchChip({ isCurrent: true })).toEqual({ kind: 'in-use', label: '使用中' });
  });

  it('lists bindings as agent + one short status', () => {
    const wallet = sampleWallet();
    expect(formatTicketBindingDetailLines(
      wallet.bindings.filter((binding) => binding.ticketId === 'provider:kimi-1'),
    )).toEqual([
      { agent: agentDisplayName('claude'), status: '当前使用' },
      { agent: 'http://127.0.0.1:8123/v1/responses', status: '本机路由运行中' },
    ]);
    expect(formatTicketBindingDetailLines(
      wallet.bindings.filter((binding) => binding.ticketId === 'account:oauth-1'),
    )).toEqual([{ agent: agentDisplayName('claude'), status: '未使用' }]);
    expect(buildTicketBindingRows(
      wallet.bindings.filter((binding) => binding.ticketId === 'provider:kimi-1'),
    )).toEqual([
      {
        agentId: 'claude',
        agentLabel: agentDisplayName('claude'),
        status: '当前使用',
        routeLabel: '改配置',
        localUrl: null,
      },
      {
        agentId: 'codex',
        agentLabel: agentDisplayName('codex'),
        status: '本机路由运行中',
        routeLabel: '本机路由',
        localUrl: 'http://127.0.0.1:8123/v1/responses',
      },
    ]);
    expect(ticketBindingStatus({
      ticketId: 'provider:kimi-1',
      agentId: 'codex',
      route: 'bridge',
      active: true,
      profileId: 'p2',
      bridge: { port: 8123, running: false },
    })).toBe('本机路由已停止');
    expect(humanizeTicketAuthLabel('可续期·未验证')).toBe('可续期');
    expect(humanizeTicketAuthLabel('已配置·未验证')).toBe('已配置');
    expect(humanizeTicketAuthLabel('可续期')).toBe('可续期');
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
    expect(extras.accountLabel).toBe('me@example.com');
    expect(extras.isCurrent).toBe(false);
    expect(extrasFromPoolSource(oauth, source, undefined, 'account:oauth-1').isCurrent).toBe(true);
    expect(extrasFromPoolSource(oauth, source, undefined, 'provider:kimi-1').isCurrent).toBe(false);
    expect(extras.canEditKey).toBe(false);
    expect(extras.canEditConfig).toBe(false);
    expect(extras.oauthAction).toEqual({ kind: 'refresh-quota', label: '刷新' });
    expect(extras.refreshTokenPreview).toBeUndefined();
    expect(ticketDetailEditLabel(extras)).toBeNull();

    const previewExtras = extrasFromPoolSource(oauth, {
      account: account({
        id: 'oauth-1',
        kind: 'oauth',
        label: 'me@example.com',
        email: 'me@example.com',
        refreshTokenPreview: 'rt--••••wxyz',
        secretTail: '**wxyz',
      }),
    });
    expect(previewExtras.refreshTokenPreview).toBe('rt--••••wxyz');
    expect(previewExtras.secretTail).toBe('**wxyz');

    const keyExtrasFromAccount = extrasFromPoolSource(
      ticket({
        id: 'account:key-1',
        sourceKind: 'account',
        sourceId: 'key-1',
        agentId: 'kimi',
        label: 'Kimi key',
        surface: 'kimi-code-membership',
        credentialClass: 'api_key',
        speaks: [],
      }),
      {
        account: account({
          id: 'key-1',
          kind: 'apikey',
          label: 'Kimi key',
          secretTail: '**here',
        }),
      },
    );
    expect(keyExtrasFromAccount.secretTail).toBe('**here');

    const keyTicket = ticket({ id: 'provider:kimi-1' });
    const keyExtras = extrasFromPoolSource(
      keyTicket,
      findTicketPoolSource(keyTicket, [], [
        provider({ id: 'kimi-1', agentId: 'kimi', name: 'Kimi 会员', secretTail: '**wxyz' }),
      ]),
    );
    expect(keyExtras.canEditConfig).toBe(true);
    expect(keyExtras.endpointHost).toBe('relay.example.com');
    expect(keyExtras.secretTail).toBe('**wxyz');
    expect(ticketDetailEditLabel(keyExtras)).toBe('编辑配置');
  });

  it('splits Grok oauth extras by Hub vs CLI ownership', () => {
    const grokTicket = ticket({
      id: 'account:grok-1',
      sourceKind: 'account',
      sourceId: 'grok-1',
      agentId: 'grok',
      label: 'user@x.ai',
      surface: 'grok-xai-subscription',
      credentialClass: 'oauth',
      speaks: [],
      importedFrom: 'grok',
    });
    expect(extrasFromPoolSource(grokTicket, {
      account: account({
        id: 'grok-1',
        agentId: 'grok',
        kind: 'oauth',
        label: 'user@x.ai',
        source: 'oauth_pkce',
        refreshable: true,
      }),
    }).oauthAction).toEqual({ kind: 'refresh-credentials', label: '刷新' });
    expect(extrasFromPoolSource(grokTicket, {
      account: account({
        id: 'grok-1',
        agentId: 'grok',
        kind: 'oauth',
        label: 'user@x.ai',
        source: 'live',
        isCurrent: true,
      }),
    }).oauthAction).toEqual({ kind: 'sync-current-login', label: '同步当前登录' });
    expect(extrasFromPoolSource(grokTicket, {
      account: account({
        id: 'grok-1',
        agentId: 'grok',
        kind: 'oauth',
        label: 'user@x.ai',
        source: 'auth.json',
        isCurrent: false,
      }),
    }).oauthAction).toEqual({ kind: 'refresh-quota', label: '刷新' });
  });
});

describe('buildTicketAddMenu', () => {
  it('nests import and API Key under each Agent', () => {
    const menu = buildTicketAddMenu(['claude', 'kimi']);
    expect(menu.map((item) => item.id)).toEqual(['claude', 'kimi']);
    expect(menu[0]?.name).toBe(agentDisplayName('claude'));
    expect(menu.map((item) => item.actions.map((a) => a.kind))).toEqual([
      ['import-login', 'oauth', 'api-key'],
      ['import-login', 'api-key'],
    ]);
    expect(menu[0]?.actions.map((a) => a.label)).toEqual([
      '导入当前登录',
      '官方登录',
      '添加 API Key',
    ]);
  });

  it('is empty when no Agent is installed', () => {
    expect(buildTicketAddMenu([])).toEqual([]);
    expect(buildTicketAddMenu(null)).toEqual([]);
    expect(buildTicketAddMenu()).toEqual([]);
  });

  it('focuses the selected Agent tab so Add skips the picker', () => {
    const menu = buildTicketAddMenu(['claude', 'kimi']);
    expect(focusedTicketAddAgent(menu, null)).toBeNull();
    expect(focusedTicketAddAgent(menu, 'kimi')?.id).toBe('kimi');
    expect(focusedTicketAddAgent(menu, 'grok')).toBeNull();
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
    expect(() => dispatchTicketAddAction('oauth', 'claude', {})).not.toThrow();
    expect(() => dispatchTicketAddAction('api-key', 'claude', {})).not.toThrow();
  });
});

describe('ticketAddDialogState', () => {
  it('opens import or API Key against the submenu Agent', () => {
    expect(ticketAddDialogState('import-login', 'codex')).toEqual({
      addAgentId: 'codex',
      loginImportOpen: true,
      oauthDialogOpen: false,
      apiKeyDialogOpen: false,
      clearEditProvider: false,
    });
    expect(ticketAddDialogState('api-key', 'grok')).toEqual({
      addAgentId: 'grok',
      loginImportOpen: false,
      oauthDialogOpen: false,
      apiKeyDialogOpen: true,
      clearEditProvider: true,
    });
    expect(ticketAddDialogState('oauth', 'claude')).toEqual({
      addAgentId: 'claude',
      loginImportOpen: false,
      oauthDialogOpen: true,
      apiKeyDialogOpen: false,
      clearEditProvider: false,
    });
  });
});

describe('handleTicketAddMenuSelect', () => {
  it('swallows the menu select, opens the matching dialog, then closes the menu', () => {
    const event = { preventDefault: vi.fn(), stopPropagation: vi.fn() };
    const onImportLogin = vi.fn();
    const onAddKey = vi.fn();
    const onMenuClose = vi.fn();
    const schedule = vi.fn<(fn: () => void, delayMs?: number) => void>();

    handleTicketAddMenuSelect(event, 'import-login', 'kimi', {
      onImportLogin,
      onAddKey,
      onMenuClose,
    }, schedule);

    expect(event.preventDefault).toHaveBeenCalledOnce();
    expect(event.stopPropagation).toHaveBeenCalledOnce();
    expect(onImportLogin).toHaveBeenCalledOnce();
    expect(onImportLogin).toHaveBeenCalledWith('kimi');
    expect(onAddKey).not.toHaveBeenCalled();
    expect(onMenuClose).not.toHaveBeenCalled();
    expect(schedule).toHaveBeenCalledOnce();
    expect(schedule).toHaveBeenCalledWith(expect.any(Function), MENU_DIALOG_DISMISS_CLEAR_MS);
    schedule.mock.calls[0]![0]();
    expect(onMenuClose).toHaveBeenCalledOnce();
  });

  it('opens the import dialog for 导入当前登录 instead of failing silently', () => {
    const event = { preventDefault: vi.fn() };
    const onImportLogin = vi.fn();
    handleTicketAddMenuSelect(event, 'import-login', 'claude', { onImportLogin });
    expect(onImportLogin).toHaveBeenCalledWith('claude');
    expect(ticketAddDialogState('import-login', 'claude')).toMatchObject({
      addAgentId: 'claude',
      loginImportOpen: true,
      apiKeyDialogOpen: false,
    });
  });

  it('opens the add API Key dialog without touching the wallet filter', () => {
    const event = { preventDefault: vi.fn() };
    const onAddKey = vi.fn();
    handleTicketAddMenuSelect(event, 'api-key', 'claude', { onAddKey });
    expect(event.preventDefault).toHaveBeenCalledOnce();
    expect(onAddKey).toHaveBeenCalledOnce();
    expect(onAddKey).toHaveBeenCalledWith('claude');
    expect(ticketAddDialogState('api-key', 'claude')).toMatchObject({
      apiKeyDialogOpen: true,
      loginImportOpen: false,
    });
  });
});

describe('ticketAddMenuClosesOnKey', () => {
  it('closes the expanded 添加登录 menu on Escape', () => {
    expect(ticketAddMenuClosesOnKey('Escape')).toBe(true);
    expect(ticketAddMenuClosesOnKey('Esc')).toBe(true);
    expect(ticketAddMenuClosesOnKey('Enter')).toBe(false);
  });
});

describe('shouldIgnoreMenuDialogDismiss', () => {
  it('ignores only a close while the opening click is still settling', () => {
    expect(shouldIgnoreMenuDialogDismiss(true, false)).toBe(true);
    expect(shouldIgnoreMenuDialogDismiss(true, true)).toBe(false);
    expect(shouldIgnoreMenuDialogDismiss(false, false)).toBe(false);
  });
});

describe('armMenuDialogOpen', () => {
  it('arms, opens, then clears after the menu-close delay', () => {
    const arm = { current: false };
    const open = vi.fn();
    const schedule = vi.fn<(fn: () => void, delayMs?: number) => void>();

    armMenuDialogOpen(arm, open, MENU_DIALOG_DISMISS_CLEAR_MS, schedule);

    expect(arm.current).toBe(true);
    expect(open).toHaveBeenCalledOnce();
    expect(schedule).toHaveBeenCalledOnce();
    expect(schedule.mock.calls[0]![1]).toBe(100);
    expect(shouldIgnoreMenuDialogDismiss(arm.current, false)).toBe(true);
    schedule.mock.calls[0]![0]();
    expect(arm.current).toBe(false);
    expect(shouldIgnoreMenuDialogDismiss(arm.current, false)).toBe(false);
  });
});

describe('handleMenuDialogSelect', () => {
  it('preventDefault then arms the same openTicketAdd ignore window', () => {
    const event = { preventDefault: vi.fn() };
    const arm = { current: false };
    const open = vi.fn();
    const schedule = vi.fn<(fn: () => void, delayMs?: number) => void>();

    handleMenuDialogSelect(event, arm, open, MENU_DIALOG_DISMISS_CLEAR_MS, schedule);

    expect(event.preventDefault).toHaveBeenCalledOnce();
    expect(open).toHaveBeenCalledOnce();
    expect(arm.current).toBe(true);
    schedule.mock.calls[0]![0]();
    expect(arm.current).toBe(false);
  });
});

describe('filter change after add-dialog leftover', () => {
  it('does not throw when returning to 全部 with mixed or incomplete tickets', () => {
    const wallet: TicketWallet = {
      tickets: [
        {
          id: 'provider:kimi-1',
          sourceKind: 'provider',
          sourceId: 'kimi-1',
          agentId: 'kimi',
          label: 'Kimi 会员',
          surface: 'kimi-code-membership',
          credentialClass: 'api_key',
          speaks: ['anthropic-messages'],
          importedFrom: 'kimi',
        },
        {
          id: 'account:codex-1',
          sourceKind: 'account',
          sourceId: 'codex-1',
          agentId: 'codex',
          label: 'ChatGPT Plus',
          surface: 'codex-chatgpt-subscription',
          credentialClass: 'oauth',
          speaks: [],
          importedFrom: 'codex',
        },
        {
          id: 'account:unknown-1',
          sourceKind: 'account',
          sourceId: 'u1',
          agentId: 'pi',
          label: '',
          surface: 'unknown',
          credentialClass: 'unknown',
          speaks: undefined as unknown as string[],
          importedFrom: null,
        },
      ],
      bindings: [
        {
          ticketId: 'account:codex-1',
          agentId: 'codex',
          route: 'native',
          active: true,
          profileId: null,
          bridge: null,
        },
        {
          ticketId: 'provider:kimi-1',
          agentId: 'claude',
          route: 'bridge',
          active: true,
          profileId: null,
          bridge: { port: null, running: false },
        },
      ],
      surfaceGroups: [],
    };

    expect(() => buildTicketWalletRows(wallet, { filter: 'api_key' })).not.toThrow();
    expect(() => buildTicketWalletRows(wallet, { filter: 'all' })).not.toThrow();
    const allRows = buildTicketWalletRows(wallet, { filter: 'all' });
    expect(allRows).toHaveLength(3);
    expect(() =>
      extrasFromPoolSource(wallet.tickets[2]!, { account: undefined, provider: undefined }),
    ).not.toThrow();
    expect(() => buildTicketDetailFields(wallet.tickets[2]!)).not.toThrow();
  });
});

describe('ticket wallet labels with translator', () => {
  it('uses kind / connections copy', () => {
    const t = createTranslator('en');
    expect(ticketWalletFilterLabel('all', t)).toBe('All');
    expect(ticketWalletFilterLabel('oauth', t)).toBe('Official login');
    expect(ticketCredentialClassChipLabel('api_key', t)).toBe('API Key');
    const wallet = sampleWallet();
    const rows = buildTicketWalletRows(wallet, { t });
    const kimi = rows.find((r) => r.ticket.id === 'provider:kimi-1');
    expect(kimi?.usageText).toContain('Rewrite config');
    expect(kimi?.usageText).toContain('Local route');
    expect(kimi?.usageText).not.toContain('改配置');
  });
});

describe('resolveTicketRouteAction', () => {
  it('stays enabled while any target is still pending', () => {
    expect(resolveTicketRouteAction([
      { status: 'pending' },
      { status: 'ready', route: 'config_sync', canApply: true },
    ])).toEqual({ disabled: false });
  });

  it('stays enabled when a local_bridge plan can apply', () => {
    expect(resolveTicketRouteAction([
      { status: 'ready', route: 'config_sync', canApply: true },
      { status: 'ready', route: 'local_bridge', canApply: true, reason: '需要本机转发' },
    ])).toEqual({ disabled: false });
  });

  it('disables with the local_bridge reason when that edge cannot apply', () => {
    expect(resolveTicketRouteAction([
      { status: 'ready', route: 'native_endpoint', canApply: true, reason: '会改配置' },
      {
        status: 'ready',
        route: 'local_bridge',
        canApply: false,
        reason: 'Claude 订阅接到 Codex 可以走本机转发，但规则还没做完',
      },
    ])).toEqual({
      disabled: true,
      reason: 'Claude 订阅接到 Codex 可以走本机转发，但规则还没做完',
    });
  });

  it('disables with the oauth reason when login is incomplete', () => {
    expect(resolveTicketRouteAction([
      { status: 'blocked_oauth', reason: '这份官方登录还没完成，请先完成登录。' },
    ])).toEqual({
      disabled: true,
      reason: '这份官方登录还没完成，请先完成登录。',
    });
  });

  it('disables with a generic reason when no target is local_bridge', () => {
    const t = createTranslator('zh');
    expect(resolveTicketRouteAction([
      { status: 'ready', route: 'native_endpoint', canApply: true, reason: '会改配置' },
      { status: 'ready', route: 'unsupported', canApply: false, reason: '这个工具不能写入' },
    ], t)).toEqual({
      disabled: true,
      reason: '这份登录目前不能走本机转发',
    });
  });

  it('stays enabled when every target failed to plan', () => {
    expect(resolveTicketRouteAction([
      { status: 'error', reason: 'plan failed' },
      { status: 'error', reason: 'timeout' },
    ])).toEqual({ disabled: false });
  });

  it('disables when there is no target agent', () => {
    expect(resolveTicketRouteAction([])).toEqual({
      disabled: true,
      reason: '没有可转发的目标工具',
    });
  });
});

describe('resolveTicketShareAction', () => {
  it('stays enabled when a direct or config-sync plan can apply', () => {
    expect(resolveTicketShareAction([
      { status: 'ready', route: 'config_sync', canApply: true, reason: '会改配置' },
      { status: 'ready', route: 'local_bridge', canApply: false, reason: '需要本机转发' },
    ])).toEqual({ disabled: false });
  });

  it('disables when the login can only local-forward', () => {
    const t = createTranslator('zh');
    expect(resolveTicketShareAction([
      { status: 'ready', route: 'local_bridge', canApply: true, reason: '需要本机转发' },
      { status: 'ready', route: 'unsupported', canApply: false },
    ], t)).toEqual({
      disabled: true,
      reason: '这份登录目前不能直接用到其它工具',
    });
  });

  it('disables with the matching write-gate reason', () => {
    expect(resolveTicketShareAction([
      { status: 'ready', route: 'config_sync', canApply: false, reason: '目标有槽、写入未开' },
    ])).toEqual({
      disabled: true,
      reason: '目标有槽、写入未开',
    });
  });
});

describe('row action disable reasons', () => {
  it('explains in-use and busy switch', () => {
    expect(ticketSwitchDisabledReason({ kind: 'in-use', switchBusy: false, canSwitch: true }))
      .toBe('这份登录已在当前工具使用中');
    expect(ticketSwitchDisabledReason({ kind: 'switch', switchBusy: true, canSwitch: true }))
      .toBe('正在切换其他登录');
    expect(ticketSwitchDisabledReason({ kind: 'switch', switchBusy: false, canSwitch: true }))
      .toBeUndefined();
  });

  it('explains refresh lock', () => {
    expect(ticketRefreshDisabledReason({ refreshing: true, refreshLocked: true }))
      .toBe('刷新中…');
    expect(ticketRefreshDisabledReason({ refreshing: false, refreshLocked: true }))
      .toBe('正在刷新其他登录');
    expect(ticketRefreshDisabledReason({ refreshing: false, refreshLocked: false }))
      .toBeUndefined();
  });
});
