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
        onShareTicket() {},
        onRouteTicket() {},
        onEditTicket() {},
        onDeleteTicket() {},
      }),
    );
    expect(markup).toContain('aria-expanded="false"');
    expect(markup).toContain('用到其他工具');
    expect(markup).toContain('本机转发');
    expect(markup).not.toContain('接到…');
    expect(markup).toContain('详情');
    expect(markup).not.toContain('搜索登录或用途');
    expect(markup).not.toContain('aria-label="搜索登录"');
    expect(markup).not.toContain('aria-label="登录类型筛选"');
    expect(markup).toContain('1 份登录');
    expect(markup).not.toContain('钱包');
    expect(markup).toContain('密钥授权');
    expect(markup).toContain('var(--agent-kimi)');
    expect(markup).toMatch(/color:\s*var\(--agent-kimi\)/);
    expect(markup).not.toContain('●');
    expect(markup).not.toContain('○');
    expect(markup).not.toContain('搜票');
    expect(markup).not.toContain('张票');
    expect(markup).not.toContain('移入回收站');
    expect(markup).not.toContain('编辑配置');
    expect(markup).not.toContain('编辑 API Key');
  });

  it('puts usage on the same row and omits a second Codex label', () => {
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
        onShareTicket() {},
        onRouteTicket() {},
        onRefreshTicket() {},
        extrasForTicket: () => ({
          oauthAction: { kind: 'refresh-credentials' as const, label: '刷新' },
          identity: 'user@example.com',
        }),
        onEditTicket() {},
        onDeleteTicket() {},
      }),
    );
    expect(markup).toContain('user@example.com');
    expect(markup).toContain('账号登录');
    expect(markup).not.toContain('aria-label="刷新"');
    expect(markup).toContain('var(--agent-codex)');
    expect(markup).toMatch(/color:\s*var\(--agent-codex\)/);
    expect(markup).toContain('Codex（切换）');
    expect(markup).not.toContain('正用于：');
    expect(markup).not.toContain('mt-1 pl-5');
  });

  it('links 本机路由 usage to /routes without opening ConnectFlow', () => {
    const wallet = sampleWallet();
    wallet.bindings = [{
      ticketId: 'provider:kimi-1',
      agentId: 'codex',
      route: 'bridge',
      active: true,
      profileId: 'bridge-1',
      bridge: { port: 43121, running: true },
    }];
    const markup = renderWithTooltip(
      createElement(TicketWalletList, {
        wallet,
        onShareTicket() {},
        onRouteTicket() {},
        onEditTicket() {},
        onDeleteTicket() {},
      }),
    );
    expect(markup).toContain('href="/routes?profile=bridge-1"');
    expect(markup).toContain('本机路由');
    expect(markup).toContain('用到其他工具');
    expect(markup).toContain('本机转发');
    expect(markup).not.toContain('接到…');
  });

  it('shows N-member poll-pool copy on the bound ticket usage line', () => {
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
        onShareTicket() {},
        onRouteTicket() {},
        onEditTicket() {},
        onDeleteTicket() {},
      }),
    );
    expect(markup).toContain('2 份同类登录可轮换');
    expect(markup).toContain('本机路由');
    expect(markup).toContain('运行中');
    expect(markup).not.toContain('sk-');
  });

  it('shows ListSkeleton while the wallet is loading', () => {
    const markup = renderWithTooltip(
      createElement(TicketWalletList, {
        wallet: null,
        loading: true,
        onShareTicket() {},
        onRouteTicket() {},
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
        onShareTicket() {},
        onRouteTicket() {},
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
        onShareTicket() {},
        onRouteTicket() {},
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
        onShareTicket() {},
        onRouteTicket() {},
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
        onShareTicket() {},
        onRouteTicket() {},
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
        onShareTicket() {},
        onRouteTicket() {},
        onEditTicket() {},
        onDeleteTicket() {},
      }),
    );
    expect(allMarkup).toContain('41375197@qq.com');
    expect(allMarkup).toContain('cunsen.chen@gmail.com');
    expect(allMarkup).toContain('2 份登录');
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

    const claudeMarkup = renderWithTooltip(
      createElement(TicketWalletList, {
        wallet,
        agentFilterId: 'claude',
        onShareTicket() {},
        onRouteTicket() {},
        onEditTicket() {},
        onDeleteTicket() {},
      }),
    );
    expect(claudeMarkup).toContain('me@openai.com');
    expect(claudeMarkup).not.toContain('user@x.ai');
    expect(claudeMarkup).toContain('1 份登录');
    expect(claudeMarkup).not.toContain('2 份登录');
    expect(claudeMarkup).not.toContain('没有匹配的登录');

    const grokMarkup = renderWithTooltip(
      createElement(TicketWalletList, {
        wallet,
        agentFilterId: 'grok',
        onShareTicket() {},
        onRouteTicket() {},
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
        onShareTicket() {},
        onRouteTicket() {},
        onEditTicket() {},
        onDeleteTicket() {},
      }),
    );
    expect(allMarkup).toContain('user@x.ai');
    expect(allMarkup).toContain('me@openai.com');
    expect(allMarkup).toContain('2 份登录');
  });

  it('does not put 添加 in the list chrome when logins exist', () => {
    const markup = renderWithTooltip(
      createElement(TicketWalletList, {
        wallet: sampleWallet(),
        installedAgentIds: ['claude'],
        onShareTicket() {},
        onRouteTicket() {},
        onEditTicket() {},
        onDeleteTicket() {},
        onAddKey() {},
        onImportLogin() {},
      }),
    );
    expect(markup).not.toContain('添加');
    expect(markup).not.toContain('aria-label="登录类型筛选"');
    expect(markup).not.toContain('新 API Key');
  });

  it('keeps 添加 on the empty-wallet next action', () => {
    const markup = renderWithTooltip(
      createElement(TicketWalletList, {
        wallet: { tickets: [], bindings: [], surfaceGroups: [] },
        installedAgentIds: ['claude'],
        onShareTicket() {},
        onRouteTicket() {},
        onEditTicket() {},
        onDeleteTicket() {},
        onAddKey() {},
        onImportLogin() {},
      }),
    );
    expect(markup).toContain('添加');
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
          onShareTicket() {},
        onRouteTicket() {},
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
          onShareTicket() {},
        onRouteTicket() {},
          onEditTicket() {},
          onDeleteTicket() {},
        }),
      ),
    ).not.toThrow();

    const allMarkup = renderWithTooltip(
      createElement(TicketWalletList, {
        wallet: mixed,
        agentFilterId: null,
        onShareTicket() {},
        onRouteTicket() {},
        onEditTicket() {},
        onDeleteTicket() {},
      }),
    );
    expect(allMarkup).toContain('Kimi 会员');
    expect(allMarkup).toContain('ChatGPT Plus');
  });
});

describe('TicketDetailPanel', () => {
  it('lays out 用量 and collapsed 更多 without 用在哪, 导入自, or header duplicates', () => {
    const markup = renderWithTooltip(
      createElement(TicketDetailPanel, {
        id: 'ticket-detail',
        advanced: [
          { label: '协议', value: 'anthropic-messages' },
        ],
        extras: { quota7dPct: 40, quota7dResetIn: '3d', canEditConfig: true, isCurrent: true },
        editLabel: '编辑配置',
        onEdit() {},
        onDelete() {},
      }),
    );
    expect(markup).toContain('id="ticket-detail"');
    expect(markup).toContain('用量');
    expect(markup).not.toContain('用在哪');
    expect(markup).not.toContain('导入自');
    expect(markup).toContain('更多');
    expect(markup).toContain('<details>');
    expect(markup).not.toContain('<details open');
    expect(markup).not.toContain('正用于');
    expect(markup).not.toContain('Claude · 改配置 · 当前');
    expect(markup).not.toContain('>类型<');
    expect(markup).not.toContain('>来源<');
    expect(markup).not.toContain('>所属<');
    expect(markup).not.toContain('官方账号');
    expect(markup).toContain('编辑配置');
    expect(markup).toContain('移入回收站');
    expect(markup).toContain('>7d<');
    expect(markup).not.toContain('>5h<');
    const moreIndex = markup.indexOf('更多');
    const protocolIndex = markup.indexOf('anthropic-messages');
    expect(moreIndex).toBeGreaterThan(-1);
    expect(protocolIndex).toBeGreaterThan(moreIndex);
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
});

describe('TicketWalletList switch action', () => {
  it('hides native 切换 when the card is on another Agent tab', () => {
    const markup = renderWithTooltip(
      createElement(TicketWalletList, {
        wallet: sampleWallet(),
        agentFilterId: 'codex',
        extrasForTicket: () => ({ isCurrent: false }),
        onShareTicket() {},
        onRouteTicket() {},
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
        onShareTicket() {},
        onRouteTicket() {},
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
        onShareTicket() {},
        onRouteTicket() {},
        onSwitchTicket() {},
        onEditTicket() {},
        onDeleteTicket() {},
      }),
    );
    expect(markup).toContain('aria-label="使用中"');
    expect(markup).toContain('>使用中<');
    expect(markup).toMatch(/disabled[^>]*aria-label="使用中"/);
    expect(markup).not.toContain('aria-label="切换"');
  });
});

describe('TicketWalletList header health chip', () => {
  it('shows a quiet 可续期 chip and never 未验证', () => {
    const markup = renderWithTooltip(
      createElement(TicketWalletList, {
        wallet: sampleWallet(),
        extrasForTicket: () => ({ authLabel: '可续期·未验证' }),
        onShareTicket() {},
        onRouteTicket() {},
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
        onShareTicket() {},
        onRouteTicket() {},
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
        onShareTicket() {},
        onRouteTicket() {},
        onEditTicket() {},
        onDeleteTicket() {},
      }),
    );
    expect(markup).toContain('**wxyz');
    expect(markup).not.toContain('已配置');
  });
});
