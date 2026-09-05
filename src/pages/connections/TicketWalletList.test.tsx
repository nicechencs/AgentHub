import { createElement, type ReactElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { MemoryRouter } from 'react-router-dom';
import { describe, expect, it } from 'vitest';
import { TooltipProvider } from '@/components/ui/tooltip';
import type { TicketWallet } from '@/lib/backend/contracts/ticket';
import { TicketAddMenu, TicketDetailPanel, TicketWalletList } from './TicketWalletList';
import { buildTicketAddMenu } from './ticket-wallet-model';

function renderWithTooltip(node: ReactElement): string {
  return renderToStaticMarkup(
    createElement(MemoryRouter, null, createElement(TooltipProvider, null, node)),
  );
}

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
        speaks: ['anthropic-messages'],
        importedFrom: 'kimi',
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
    ],
    surfaceGroups: [],
  };
}

describe('TicketWalletList details', () => {
  it('keeps the row collapsed so 详情 is not an edit dialog', () => {
    const markup = renderWithTooltip(
      createElement(TicketWalletList, {
        wallet: sampleWallet(),
        onImportToPool() {},
        onEditTicket() {},
        onDeleteTicket() {},
        onShowDetail() {},
      }),
    );
    expect(markup).not.toContain('aria-expanded');
    expect(markup).not.toContain('分享至连接池');
    expect(markup).not.toContain('用到其他工具');
    expect(markup).not.toContain('本机转发');
    expect(markup).not.toContain('接到…');
    expect(markup).not.toContain('详情');
    expect(markup).not.toContain('搜索登录或用途');
    expect(markup).not.toContain('aria-label="搜索登录"');
    expect(markup).not.toContain('aria-label="登录类型筛选"');
    expect(markup).toContain('1 份登录');
    expect(markup).not.toContain('钱包');
    expect(markup).toContain('API Key');
    expect(markup).toContain('var(--agent-kimi)');
    expect(markup).toMatch(/color:\s*var\(--agent-kimi\)/);
    expect(markup).not.toContain('●');
    expect(markup).not.toContain('○');
    expect(markup).not.toContain('搜票');
    expect(markup).not.toContain('张票');
    expect(markup).not.toContain('账号登录');
    expect(markup).not.toContain('密钥授权');
    expect(markup).not.toContain('Ticket');
    expect(markup).not.toContain('wallet');
    expect(markup).not.toContain('Adapter');
    expect(markup).not.toContain('loopback');
    expect(markup).not.toContain('PKCE');
    expect(markup).not.toContain('投影');
    expect(markup).not.toContain('真源');
    expect(markup).not.toContain('移入回收站');
    expect(markup).not.toContain('编辑配置');
    expect(markup).not.toContain('编辑 API Key');
    expect(markup).toContain('<table');
    expect(markup).toContain('data-col="login"');
    expect(markup).toContain('data-col="kind"');
    expect(markup).toContain('data-col="status"');
    expect(markup).toContain('data-col="lastUsed"');
    expect(markup).toContain('data-col="usage"');
    expect(markup).toContain('data-col="agent"');
    expect(markup).toContain('data-col="actions"');
    expect(markup).toContain('最近使用');
    expect(markup).toContain('用量');
    expect(markup).toContain('data-table-layout="split"');
    expect(markup).toContain('data-ticket-name="provider:kimi-1"');
    expect(markup).not.toMatch(/<tr[^>]*tabindex="0"/);
  });

  it('marks the inspected ticket as selected', () => {
    const markup = renderWithTooltip(
      createElement(TicketWalletList, {
        wallet: sampleWallet(),
        onImportToPool() {},
        onEditTicket() {},
        onDeleteTicket() {},
        activeTicketId: 'provider:kimi-1',
      }),
    );
    expect(markup).toContain('data-active="true"');
    expect(markup).toContain('data-ticket-row="provider:kimi-1"');
  });

  it('shows a reorder handle when there are two logins', () => {
    const wallet = sampleWallet();
    wallet.tickets = [
      wallet.tickets[0]!,
      {
        ...wallet.tickets[0]!,
        id: 'provider:kimi-2',
        sourceId: 'kimi-2',
        label: 'Kimi 备用',
      },
    ];
    const markup = renderWithTooltip(
      createElement(TicketWalletList, {
        wallet,
        onImportToPool() {},
        onEditTicket() {},
        onDeleteTicket() {},
      }),
    );
    expect(markup).toContain('拖动排序');
    expect(markup).toContain('data-sortable-id');
  });

  it('puts 编辑配置 on the collapsed card, not only inside 详情', () => {
    const markup = renderWithTooltip(
      createElement(TicketWalletList, {
        wallet: sampleWallet(),
        extrasForTicket: () => ({ canEditConfig: true }),
        onImportToPool() {},
        onEditTicket() {},
        onDeleteTicket() {},
      }),
    );
    expect(markup).not.toContain('aria-expanded');
    expect(markup).toContain('编辑配置');
    expect(markup).not.toContain('移入回收站');
  });

  it('does not show an OpenAI product chip on an OpenRouter API Key card', () => {
    const wallet: TicketWallet = {
      tickets: [{
        id: 'provider:or-1',
        sourceKind: 'provider',
        sourceId: 'or-1',
        agentId: 'claude',
        label: 'OpenRouter',
        surface: 'openai-api',
        credentialClass: 'api_key',
        speaks: ['openai-chat'],
        importedFrom: null,
      }],
      bindings: [],
      surfaceGroups: [],
    };
    const markup = renderWithTooltip(
      createElement(TicketWalletList, {
        wallet,
        onImportToPool() {},
        onEditTicket() {},
        onDeleteTicket() {},
      }),
    );
    expect(markup).toContain('OpenRouter');
    expect(markup).not.toContain('>OpenAI<');
  });

  it('shows Agent on the same row and omits a second Codex label', () => {
    const wallet: TicketWallet = {
      tickets: [{
        id: 'account:codex-1',
        sourceKind: 'account',
        sourceId: 'codex-1',
        agentId: 'codex',
        label: 'ChatGPT Plus',
        surface: 'codex-chatgpt-subscription',
        credentialClass: 'oauth',
        speaks: ['openai-responses'],
        importedFrom: 'codex',
      }],
      bindings: [{
        ticketId: 'account:codex-1',
        agentId: 'codex',
        route: 'native',
        active: true,
        profileId: null,
        bridge: null,
      }],
      surfaceGroups: [],
    };
    const markup = renderWithTooltip(
      createElement(TicketWalletList, {
        wallet,
        onImportToPool() {},
        extrasForTicket: () => ({
          oauthAction: { kind: 'refresh-credentials' as const, label: '刷新' },
          identity: 'user@example.com',
        }),
        onEditTicket() {},
        onDeleteTicket() {},
      }),
    );
    expect(markup).toContain('user@example.com');
    expect(markup).toContain('官方登录');
    expect(markup).not.toContain('aria-label="刷新"');
    expect(markup).toContain('var(--agent-codex)');
    expect(markup).toMatch(/color:\s*var\(--agent-codex\)/);
    expect(markup).toContain('Codex');
    expect(markup).toContain('data-col="agent"');
    expect(markup).not.toContain('（直连）');
    expect(markup).not.toContain('(直连)');
    expect(markup).not.toContain('正用于：');
    expect(markup).not.toContain('mt-1 pl-5');
  });

  it('shows last used and usage percents on the row', () => {
    const wallet = sampleWallet();
    wallet.tickets = [{
      id: 'account:codex-1',
      sourceKind: 'account',
      sourceId: 'codex-1',
      agentId: 'codex',
      label: 'ChatGPT Plus',
      surface: 'codex-chatgpt-subscription',
      credentialClass: 'oauth',
      speaks: ['openai-responses'],
      importedFrom: 'codex',
    }];
    wallet.bindings = [];
    const markup = renderWithTooltip(
      createElement(TicketWalletList, {
        wallet,
        onImportToPool() {},
        extrasForTicket: () => ({
          lastUsedAt: '2026-08-28T08:00:00.000Z',
          quota7dPct: 40,
          quota5hPct: 12,
        }),
        onEditTicket() {},
        onDeleteTicket() {},
      }),
    );
    expect(markup).toContain('7d 40%');
    expect(markup).toContain('5h 12%');
    expect(markup).toMatch(/\d{4}-\d{2}-\d{2} \d{2}:\d{2}/);
  });

  it('does not put 本机路由 or pool rotation copy on the Agent column', () => {
    const wallet = sampleWallet();
    wallet.bindings = [{
      ticketId: 'provider:kimi-1',
      agentId: 'codex',
      route: 'bridge',
      active: true,
      profileId: 'bridge-1',
      bridge: { port: 43121, running: true },
    }];
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
    const markup = renderWithTooltip(
      createElement(TicketWalletList, {
        wallet,
        onImportToPool() {},
        onEditTicket() {},
        onDeleteTicket() {},
      }),
    );
    expect(markup).toContain('data-col="agent"');
    expect(markup).toContain('Kimi');
    expect(markup).not.toContain('href="/routes/pool?profile=bridge-1"');
    expect(markup).not.toContain('本机路由');
    expect(markup).not.toContain('2 份同类登录可轮换');
    expect(markup).not.toContain('分享至连接池');
    expect(markup).not.toContain('用到其他工具');
    expect(markup).not.toContain('本机转发');
    expect(markup).not.toContain('接到…');
    expect(markup).not.toContain('sk-');
  });

  it('shows ListSkeleton while the wallet is loading', () => {
    const markup = renderWithTooltip(
      createElement(TicketWalletList, {
        wallet: null,
        loading: true,
        onImportToPool() {},
        onEditTicket() {},
        onDeleteTicket() {},
      }),
    );
    expect(markup).toContain('animate-pulse');
    expect(markup).not.toContain('正在加载钱包');
  });

  it('uses 登录 copy for an empty wallet', () => {
    const markup = renderWithTooltip(
      createElement(TicketWalletList, {
        wallet: { tickets: [], bindings: [], surfaceGroups: [] },
        onImportToPool() {},
        onEditTicket() {},
        onDeleteTicket() {},
      }),
    );
    expect(markup).toContain('还没有登录');
    expect(markup).toContain('0 份登录');
    expect(markup).not.toContain('钱包');
    expect(markup).not.toContain('aria-label="登录类型筛选"');
    expect(markup).not.toContain('钱包还没有票');
  });

  it('uses 登录 copy when filters match nothing', () => {
    const markup = renderWithTooltip(
      createElement(TicketWalletList, {
        wallet: sampleWallet(),
        agentFilterId: 'grok',
        onImportToPool() {},
        onEditTicket() {},
        onDeleteTicket() {},
      }),
    );
    expect(markup).toContain('没有匹配的登录');
    expect(markup).toContain('0 份登录');
    expect(markup).not.toContain('1 份登录');
    expect(markup).not.toContain('钱包');
    expect(markup).not.toContain('没有匹配的票');
  });

  it('filters cards to the selected agent and matches the footer count', () => {
    const wallet: TicketWallet = {
      tickets: [
        {
          id: 'account:claude-1',
          sourceKind: 'account',
          sourceId: 'claude-1',
          agentId: 'claude',
          label: '41375197@qq.com',
          surface: 'claude-subscription',
          credentialClass: 'oauth',
          speaks: [],
          importedFrom: 'claude',
        },
        {
          id: 'account:grok-1',
          sourceKind: 'account',
          sourceId: 'grok-1',
          agentId: 'grok',
          label: 'cunsen.chen@gmail.com',
          surface: 'grok-xai-subscription',
          credentialClass: 'oauth',
          speaks: [],
          importedFrom: 'grok',
        },
      ],
      bindings: [
        {
          ticketId: 'account:claude-1',
          agentId: 'claude',
          route: 'native',
          active: true,
          profileId: null,
          bridge: null,
        },
        {
          ticketId: 'account:claude-1',
          agentId: 'codex',
          route: 'bridge',
          active: true,
          profileId: 'p-codex',
          bridge: { port: 33923, running: true },
        },
        {
          ticketId: 'account:grok-1',
          agentId: 'grok',
          route: 'native',
          active: true,
          profileId: null,
          bridge: null,
        },
      ],
      surfaceGroups: [],
    };
    const claudeMarkup = renderWithTooltip(
      createElement(TicketWalletList, {
        wallet,
        agentFilterId: 'claude',
        onImportToPool() {},
        onEditTicket() {},
        onDeleteTicket() {},
      }),
    );
    expect(claudeMarkup).toContain('41375197@qq.com');
    expect(claudeMarkup).not.toContain('cunsen.chen@gmail.com');
    expect(claudeMarkup).toContain('1 份登录');
    expect(claudeMarkup).not.toContain('2 份登录');

    const grokMarkup = renderWithTooltip(
      createElement(TicketWalletList, {
        wallet,
        agentFilterId: 'grok',
        onImportToPool() {},
        onEditTicket() {},
        onDeleteTicket() {},
      }),
    );
    expect(grokMarkup).toContain('cunsen.chen@gmail.com');
    expect(grokMarkup).not.toContain('41375197@qq.com');
    expect(grokMarkup).toContain('1 份登录');

    const allMarkup = renderWithTooltip(
      createElement(TicketWalletList, {
        wallet,
        onImportToPool() {},
        onEditTicket() {},
        onDeleteTicket() {},
      }),
    );
    expect(allMarkup).toContain('41375197@qq.com');
    expect(allMarkup).toContain('cunsen.chen@gmail.com');
    expect(allMarkup).toContain('2 份登录');
  });

  it('counts each login once under its owner agent so tab chips sum to the total', () => {
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

    const claudeMarkup = renderWithTooltip(
      createElement(TicketWalletList, {
        wallet,
        agentFilterId: 'claude',
        onImportToPool() {},
        onEditTicket() {},
        onDeleteTicket() {},
      }),
    );
    expect(claudeMarkup).not.toContain('me@openai.com');
    expect(claudeMarkup).not.toContain('user@x.ai');
    expect(claudeMarkup).toContain('0 份登录');
    expect(claudeMarkup).not.toContain('1 份登录');

    const grokMarkup = renderWithTooltip(
      createElement(TicketWalletList, {
        wallet,
        agentFilterId: 'grok',
        onImportToPool() {},
        onEditTicket() {},
        onDeleteTicket() {},
      }),
    );
    expect(grokMarkup).toContain('user@x.ai');
    expect(grokMarkup).not.toContain('me@openai.com');
    expect(grokMarkup).toContain('1 份登录');

    const allMarkup = renderWithTooltip(
      createElement(TicketWalletList, {
        wallet,
        onImportToPool() {},
        onEditTicket() {},
        onDeleteTicket() {},
      }),
    );
    expect(allMarkup).toContain('user@x.ai');
    expect(allMarkup).toContain('me@openai.com');
    expect(allMarkup).toContain('2 份登录');
  });

  it('does not put 添加授权 in the list chrome when logins exist', () => {
    const markup = renderWithTooltip(
      createElement(TicketWalletList, {
        wallet: sampleWallet(),
        installedAgentIds: ['claude'],
        onImportToPool() {},
        onEditTicket() {},
        onDeleteTicket() {},
        onAddKey() {},
        onImportLogin() {},
      }),
    );
    expect(markup).not.toContain('添加授权');
    expect(markup).not.toContain('aria-label="登录类型筛选"');
    expect(markup).not.toContain('新 API Key');
  });

  it('keeps 添加授权 on the empty-wallet next action', () => {
    const markup = renderWithTooltip(
      createElement(TicketWalletList, {
        wallet: { tickets: [], bindings: [], surfaceGroups: [] },
        installedAgentIds: ['claude'],
        onImportToPool() {},
        onEditTicket() {},
        onDeleteTicket() {},
        onAddKey() {},
        onImportLogin() {},
      }),
    );
    expect(markup).toContain('添加授权');
    expect(markup).toContain('还没有登录');
  });

  it('renders a focused Agent Add menu without throwing', () => {
    expect(() =>
      renderWithTooltip(
        createElement(TicketAddMenu, {
          agents: buildTicketAddMenu(['claude', 'kimi']),
          focusedAgentId: 'kimi',
          onAddKey() {},
          onImportLogin() {},
        }),
      ),
    ).not.toThrow();
  });

  it('renders 全部 after an API Key filter without throwing', () => {
    const mixed: TicketWallet = {
      tickets: [
        ...sampleWallet().tickets,
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
          speaks: [],
          importedFrom: null,
        },
      ],
      bindings: [
        ...sampleWallet().bindings,
        {
          ticketId: 'account:codex-1',
          agentId: 'codex',
          route: 'native',
          active: true,
          profileId: null,
          bridge: null,
        },
      ],
      surfaceGroups: [],
    };

    expect(() =>
      renderWithTooltip(
        createElement(TicketWalletList, {
          wallet: mixed,
          agentFilterId: 'kimi',
          onImportToPool() {},
          onEditTicket() {},
          onDeleteTicket() {},
        }),
      ),
    ).not.toThrow();

    expect(() =>
      renderWithTooltip(
        createElement(TicketWalletList, {
          wallet: mixed,
          agentFilterId: null,
          onImportToPool() {},
          onEditTicket() {},
          onDeleteTicket() {},
        }),
      ),
    ).not.toThrow();

    const allMarkup = renderWithTooltip(
      createElement(TicketWalletList, {
        wallet: mixed,
        agentFilterId: null,
        onImportToPool() {},
        onEditTicket() {},
        onDeleteTicket() {},
      }),
    );
    expect(allMarkup).toContain('Kimi 会员');
    expect(allMarkup).toContain('ChatGPT Plus');
  });
});

describe('TicketDetailPanel', () => {
  it('lays out 用量 and advanced fields without 更多, 用在哪, 导入自, or header duplicates', () => {
    const markup = renderWithTooltip(
      createElement(TicketDetailPanel, {
        id: 'ticket-detail',
        advanced: [
          { label: '协议', value: 'anthropic-messages' },
        ],
        extras: { quota7dPct: 40, quota7dResetIn: '3d', canEditConfig: true, isCurrent: true },
        onDelete() {},
      }),
    );
    expect(markup).toContain('id="ticket-detail"');
    expect(markup).toContain('用量');
    expect(markup).not.toContain('用在哪');
    expect(markup).not.toContain('导入自');
    expect(markup).not.toContain('更多');
    expect(markup).not.toContain('<details>');
    expect(markup).toContain('anthropic-messages');
    expect(markup).not.toContain('正用于');
    expect(markup).not.toContain('Claude · 改配置 · 当前');
    expect(markup).not.toContain('>类型<');
    expect(markup).not.toContain('>来源<');
    expect(markup).not.toContain('>所属<');
    expect(markup).not.toContain('官方账号');
    expect(markup).not.toContain('编辑配置');
    expect(markup).toContain('移入回收站');
    expect(markup).toContain('>7d<');
    expect(markup).not.toContain('>5h<');
    const usageIndex = markup.indexOf('用量');
    const protocolIndex = markup.indexOf('anthropic-messages');
    expect(usageIndex).toBeGreaterThan(-1);
    expect(protocolIndex).toBeGreaterThan(usageIndex);
  });

  it('shows the Codex 5h quota bar only when upstream returned it', () => {
    const with5h = renderWithTooltip(
      createElement(TicketDetailPanel, {
        id: 'quota-5h',
        advanced: [],
        extras: {
          quota7dPct: 89,
          quota7dResetIn: '3d11h 后重置',
          quota5hPct: 12,
          quotaResetIn: '4h20m 后重置',
        },
        onDelete() {},
      }),
    );
    expect(with5h).toContain('>7d<');
    expect(with5h).toContain('>5h<');
    expect(with5h).toContain('12%');
    expect(with5h).toContain('4h20m 后重置');

    const only7d = renderWithTooltip(
      createElement(TicketDetailPanel, {
        id: 'quota-7d-only',
        advanced: [],
        extras: { quota7dPct: 89, quota7dResetIn: '3d11h 后重置' },
        onDelete() {},
      }),
    );
    expect(only7d).toContain('>7d<');
    expect(only7d).not.toContain('>5h<');
  });

  it('puts refresh in details, not on the card', () => {
    const markup = renderWithTooltip(
      createElement(TicketDetailPanel, {
        id: 'oauth-refresh-detail',
        advanced: [],
        extras: { oauthAction: { kind: 'refresh-credentials', label: '刷新' } },
        onRefresh() {},
        onDelete() {},
      }),
    );
    expect(markup).toContain('aria-label="刷新"');
    expect(markup).toContain('刷新');
    expect(markup).toContain('移入回收站');
  });

  it('labels sync-current-login as 同步当前登录', () => {
    const markup = renderWithTooltip(
      createElement(TicketDetailPanel, {
        id: 'oauth-sync-detail',
        advanced: [],
        extras: { oauthAction: { kind: 'sync-current-login', label: '同步当前登录' } },
        onRefresh() {},
        onDelete() {},
      }),
    );
    expect(markup).toContain('aria-label="同步当前登录"');
    expect(markup).toContain('同步当前登录');
  });

  it('shows 同步中… while sync-current-login is in flight', () => {
    const markup = renderWithTooltip(
      createElement(TicketDetailPanel, {
        id: 'oauth-sync-busy',
        advanced: [],
        extras: { oauthAction: { kind: 'sync-current-login', label: '同步当前登录' } },
        refreshing: true,
        onRefresh() {},
        onDelete() {},
      }),
    );
    expect(markup).toContain('同步中…');
    expect(markup).not.toContain('刷新中…');
  });

  it('shows a redacted refresh token for OAuth details and never the full secret', () => {
    const secret = 'rt-abcdefghijklmnopqrstuvwxyz';
    const markup = renderWithTooltip(
      createElement(TicketDetailPanel, {
        id: 'oauth-rt-detail',
        advanced: [],
        extras: { refreshTokenPreview: 'rt--••••wxyz' },
        onDelete() {},
      }),
    );
    expect(markup).toContain('续期凭证');
    expect(markup).toContain('rt--••••wxyz');
    expect(markup).not.toContain(secret);
    expect(markup).not.toContain('导入自');
    expect(markup).toContain('font-mono');
  });

  it('omits 更多 for official login when only import + auth remain', () => {
    const markup = renderWithTooltip(
      createElement(TicketDetailPanel, {
        id: 'oauth-grok-detail',
        advanced: [],
        extras: { authLabel: '可续期·未验证' },
        onDelete() {},
      }),
    );
    expect(markup).not.toContain('用在哪');
    expect(markup).not.toContain('导入自');
    expect(markup).toContain('移入回收站');
    expect(markup).not.toContain('用量');
    expect(markup).not.toContain('更多');
    expect(markup).not.toContain('<details>');
    expect(markup).not.toContain('登录状态');
    expect(markup).not.toContain('尚未验证');
    expect(markup).not.toContain('未验证');
    expect(markup).not.toContain('可续期·未验证');
  });

  it('omits 直连 from 接到 rows', () => {
    const ticket = sampleWallet().tickets[0]!;
    const markup = renderWithTooltip(
      createElement(TicketDetailPanel, {
        id: 'native-clients',
        asPanel: true,
        open: true,
        ticket,
        extras: {},
        bindings: [{
          ticketId: ticket.id,
          agentId: 'claude',
          route: 'native',
          active: true,
          profileId: null,
          bridge: null,
        }],
        onDelete() {},
        onOpenChange() {},
      }),
    );
    expect(markup).toContain('接到');
    expect(markup).toContain('Claude');
    expect(markup).toContain('当前使用');
    expect(markup).not.toContain('直连');
    expect(markup).not.toContain('Direct');
  });

  it('opens as a right-hand inspect pane with clients, protocol, and diagnostics', () => {
    const ticket = sampleWallet().tickets[0]!;
    const markup = renderWithTooltip(
      createElement(TicketDetailPanel, {
        id: 'ticket-detail-pane',
        asPanel: true,
        open: true,
        ticket,
        extras: { canEditConfig: true, endpointMode: 'custom', endpointHost: 'https://relay.example.com/v1' },
        bindings: [{
          ticketId: ticket.id,
          agentId: 'codex',
          route: 'bridge',
          active: true,
          profileId: 'bridge-1',
          bridge: { port: 43121, running: true },
        }],
        onDelete() {},
        onEdit() {},
        onOpenChange() {},
      }),
    );
    expect(markup).toContain('data-side-inspect');
    expect(markup).toContain('登录详情');
    expect(markup).not.toContain('取消');
    expect(markup).toContain('收起');
    expect(markup).toContain('编辑配置');
    expect(markup).toContain('接到');
    expect(markup).toContain('Codex');
    expect(markup).toContain('本机路由运行中');
    expect(markup).toContain('http://127.0.0.1:43121');
    expect(markup).toContain('接口');
    expect(markup).toContain('Claude');
    expect(markup).toContain('诊断信息');
    expect(markup).toContain('provider:kimi-1');
    expect(markup).toContain('移入回收站');
    expect(markup.indexOf('移入回收站')).toBeLessThan(markup.indexOf('编辑配置'));
    expect(markup.indexOf('编辑配置')).toBeLessThan(markup.indexOf('收起'));
    expect(markup).not.toContain('justify-start gap-2 border-t');
    expect(markup).not.toContain('role="dialog"');
  });

  it('puts 同步当前登录 and 移入回收站 in the inspect header without 取消', () => {
    const markup = renderWithTooltip(
      createElement(TicketDetailPanel, {
        id: 'ticket-detail-header-actions',
        asPanel: true,
        open: true,
        extras: { oauthAction: { kind: 'sync-current-login', label: '同步当前登录' } },
        onRefresh() {},
        onDelete() {},
        onOpenChange() {},
      }),
    );
    expect(markup).toContain('data-side-inspect');
    expect(markup).not.toContain('取消');
    expect(markup).toContain('同步当前登录');
    expect(markup).toContain('移入回收站');
    expect(markup).toContain('收起');
    expect(markup.indexOf('同步当前登录')).toBeLessThan(markup.indexOf('移入回收站'));
    expect(markup.indexOf('移入回收站')).toBeLessThan(markup.indexOf('收起'));
    expect(markup).not.toContain('justify-start gap-2 border-t');
  });

  it('does not offer edit for OAuth tickets', () => {
    const markup = renderWithTooltip(
      createElement(TicketDetailPanel, {
        id: 'oauth-detail',
        advanced: [],
        extras: { authLabel: '可续期·未验证' },
        onDelete() {},
      }),
    );
    expect(markup).not.toContain('用在哪');
    expect(markup).not.toContain('还没接到任何工具');
    expect(markup).not.toContain('导入自');
    expect(markup).not.toContain('登录状态');
    expect(markup).not.toContain('尚未验证');
    expect(markup).not.toContain('未验证');
    expect(markup).not.toContain('可续期·未验证');
    expect(markup).not.toContain('未验证');
    expect(markup).not.toContain('未绑定任何 Agent');
    expect(markup).not.toContain('编辑密钥');
    expect(markup).not.toContain('编辑配置');
    expect(markup).not.toContain('用量');
    expect(markup).not.toContain('更多');
  });

  it('lists associated files under login details with copy and 目录', () => {
    const ticket = {
      ...sampleWallet().tickets[0]!,
      id: 'account:grok-1',
      sourceKind: 'account' as const,
      sourceId: 'grok-1',
      agentId: 'grok' as const,
      label: 'a@example.com',
      surface: 'grok-xai-subscription' as const,
      credentialClass: 'oauth' as const,
    };
    const secret = 'xai-file-key-12345678';
    const markup = renderWithTooltip(
      createElement(TicketDetailPanel, {
        id: 'ticket-auth-files',
        asPanel: true,
        open: true,
        ticket,
        extras: {
          credentialFiles: [
            {
              name: 'auth.json',
              content: '{\n  "email": "a@example.com",\n  "refresh_token": "***"\n}\n',
            },
            {
              name: 'config.toml',
              content: 'api_key = "***"\n',
            },
          ],
        },
        onDelete() {},
        onOpenChange() {},
      }),
    );
    expect(markup).toContain('相关文件');
    expect(markup).toContain('auth.json');
    expect(markup).toContain('config.toml');
    expect(markup).toContain('a@example.com');
    expect(markup).toContain('~/.grok/auth.json');
    expect(markup).toContain('~/.grok/config.toml');
    expect(markup).toContain('aria-label="复制"');
    expect(markup).toContain('>目录<');
    expect(markup).toContain('aria-label="~/.grok/auth.json"');
    expect(markup.indexOf('auth.json')).toBeLessThan(markup.indexOf('config.toml'));
    expect(markup).not.toContain(secret);
  });
});

describe('TicketWalletList switch action', () => {
  it('hides native 切换 when the card is on another Agent tab', () => {
    const markup = renderWithTooltip(
      createElement(TicketWalletList, {
        wallet: sampleWallet(),
        agentFilterId: 'codex',
        extrasForTicket: () => ({ isCurrent: false }),
        onImportToPool() {},
        onSwitchTicket() {},
        onEditTicket() {},
        onDeleteTicket() {},
      }),
    );
    expect(markup).not.toContain('aria-label="切换"');
    expect(markup).not.toContain('aria-label="使用中"');
  });

  it('shows an enabled 切换 button for unused grants', () => {
    const markup = renderWithTooltip(
      createElement(TicketWalletList, {
        wallet: sampleWallet(),
        extrasForTicket: () => ({ isCurrent: false }),
        onImportToPool() {},
        onSwitchTicket() {},
        onEditTicket() {},
        onDeleteTicket() {},
      }),
    );
    expect(markup).toContain('aria-label="切换"');
    expect(markup).toContain('>切换<');
    expect(markup).not.toContain('aria-label="使用中"');
    expect(markup).not.toMatch(/\sdisabled(=""|\s)[^>]*aria-label="切换"/);
  });

  it('shows a disabled 使用中 button for the live grant', () => {
    const markup = renderWithTooltip(
      createElement(TicketWalletList, {
        wallet: sampleWallet(),
        extrasForTicket: () => ({ isCurrent: true }),
        onImportToPool() {},
        onSwitchTicket() {},
        onEditTicket() {},
        onDeleteTicket() {},
      }),
    );
    expect(markup).toContain('aria-label="使用中"');
    expect(markup).toContain('>使用中<');
    expect(markup).toMatch(/\sdisabled(=""|\s)[^>]*aria-label="使用中"/);
    expect(markup).toContain('这份登录已在当前工具使用中');
    expect(markup).not.toContain('aria-label="切换"');
  });

  it('shows 写入 ZCode / 已添加 for catalog-append occupancy', () => {
    const wallet: TicketWallet = {
      tickets: [
        {
          id: 'provider:zcode-1',
          sourceKind: 'provider',
          sourceId: 'zcode-1',
          agentId: 'zcode',
          label: 'Z.ai',
          surface: 'glm-coding-plan',
          credentialClass: 'api_key',
          speaks: ['anthropic-messages'],
          importedFrom: 'zcode',
        },
      ],
      bindings: [],
      surfaceGroups: [],
    };
    const idle = renderWithTooltip(
      createElement(TicketWalletList, {
        wallet,
        extrasForTicket: () => ({ isCurrent: false }),
        onImportToPool() {},
        onSwitchTicket() {},
        onEditTicket() {},
        onDeleteTicket() {},
      }),
    );
    expect(idle).toContain('aria-label="写入 ZCode"');
    expect(idle).toContain('>写入 ZCode<');
    expect(idle).not.toContain('aria-label="切换"');

    const current = renderWithTooltip(
      createElement(TicketWalletList, {
        wallet,
        extrasForTicket: () => ({ isCurrent: true }),
        onImportToPool() {},
        onSwitchTicket() {},
        onEditTicket() {},
        onDeleteTicket() {},
      }),
    );
    expect(current).toContain('aria-label="已添加"');
    expect(current).toContain('>已添加<');
    expect(current).toContain('这份登录已经添加');
    expect(current).not.toContain('aria-label="使用中"');
    expect(current).not.toContain('已在模型列表里');
  });
});

describe('TicketWalletList header health chip', () => {
  it('shows a quiet 可续期 chip and never 未验证', () => {
    const markup = renderWithTooltip(
      createElement(TicketWalletList, {
        wallet: sampleWallet(),
        extrasForTicket: () => ({ authLabel: '可续期·未验证' }),
        onImportToPool() {},
        onEditTicket() {},
        onDeleteTicket() {},
      }),
    );
    expect(markup).toContain('可续期');
    expect(markup).not.toContain('尚未验证');
    expect(markup).not.toContain('未验证');
    expect(markup).not.toContain('可续期·未验证');
  });

  it('shows the refresh-token tail instead of 可续期', () => {
    const markup = renderWithTooltip(
      createElement(TicketWalletList, {
        wallet: sampleWallet(),
        extrasForTicket: () => ({ authLabel: '可续期·未验证', secretTail: '**JF6Q' }),
        onImportToPool() {},
        onEditTicket() {},
        onDeleteTicket() {},
      }),
    );
    expect(markup).toContain('**JF6Q');
    expect(markup).toContain('font-mono');
    expect(markup).not.toContain('可续期');
    expect(markup).not.toContain('未验证');
  });

  it('shows the API key tail instead of 已配置', () => {
    const markup = renderWithTooltip(
      createElement(TicketWalletList, {
        wallet: sampleWallet(),
        extrasForTicket: () => ({ authLabel: '已配置', secretTail: '**wxyz' }),
        onImportToPool() {},
        onEditTicket() {},
        onDeleteTicket() {},
      }),
    );
    expect(markup).toContain('**wxyz');
    expect(markup).not.toContain('已配置');
  });
});
