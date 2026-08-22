import { createElement, type ReactElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { MemoryRouter } from 'react-router-dom';
import { describe, expect, it } from 'vitest';
import { TooltipProvider } from '@/components/ui/tooltip';
import type { TicketWallet } from '@/lib/backend/contracts/ticket';
import { TicketDetailPanel, TicketWalletList } from './TicketWalletList';

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
        onConnectTicket() {},
        onEditTicket() {},
        onDeleteTicket() {},
      }),
    );
    expect(markup).toContain('aria-expanded="false"');
    expect(markup).toContain('接到…');
    expect(markup).toContain('详情');
    expect(markup).toContain('搜索登录或用途');
    expect(markup).toContain('aria-label="搜索登录"');
    expect(markup).toContain('pl-8');
    expect(markup).not.toContain('pl-7');
    expect(markup).toContain('aria-label="登录类型筛选"');
    expect(markup).toContain('1 份登录');
    expect(markup).not.toContain('钱包');
    expect(markup).toContain('var(--agent-kimi)');
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
        onConnectTicket() {},
        onEditTicket() {},
        onDeleteTicket() {},
      }),
    );
    expect(markup).toContain('ChatGPT Plus');
    expect(markup).toContain('var(--agent-codex)');
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
        onConnectTicket() {},
        onEditTicket() {},
        onDeleteTicket() {},
      }),
    );
    expect(markup).toContain('href="/routes?profile=bridge-1"');
    expect(markup).toContain('本机路由');
    expect(markup).toContain('接到…');
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
        onConnectTicket() {},
        onEditTicket() {},
        onDeleteTicket() {},
      }),
    );
    expect(markup).toContain('2 个登录轮询承接');
    expect(markup).toContain('本机路由');
    expect(markup).toContain('运行中');
    expect(markup).not.toContain('sk-');
  });

  it('shows ListSkeleton while the wallet is loading', () => {
    const markup = renderWithTooltip(
      createElement(TicketWalletList, {
        wallet: null,
        loading: true,
        onConnectTicket() {},
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
        onConnectTicket() {},
        onEditTicket() {},
        onDeleteTicket() {},
      }),
    );
    expect(markup).toContain('还没有登录');
    expect(markup).toContain('0 份登录');
    expect(markup).not.toContain('钱包');
    expect(markup).toContain('官方登录');
    expect(markup).toContain('API Key');
    expect(markup).toContain('未识别');
    expect(markup).not.toContain('钱包还没有票');
  });

  it('uses 登录 copy when filters match nothing', () => {
    const markup = renderWithTooltip(
      createElement(TicketWalletList, {
        wallet: sampleWallet(),
        initialFilter: 'oauth',
        onConnectTicket() {},
        onEditTicket() {},
        onDeleteTicket() {},
      }),
    );
    expect(markup).toContain('没有匹配的登录');
    expect(markup).toContain('1 份登录');
    expect(markup).not.toContain('钱包');
    expect(markup).not.toContain('没有匹配的票');
  });

  it('keeps 添加 as a menu trigger and does not add a 新 API Key filter chip', () => {
    const markup = renderWithTooltip(
      createElement(TicketWalletList, {
        wallet: sampleWallet(),
        installedAgentIds: ['claude'],
        onConnectTicket() {},
        onEditTicket() {},
        onDeleteTicket() {},
        onAddKey() {},
        onImportLogin() {},
      }),
    );
    expect(markup).toContain('添加');
    expect(markup).toContain('aria-label="登录类型筛选"');
    expect(markup).toContain('全部');
    expect(markup).toContain('API Key');
    expect(markup).not.toContain('新 API Key');
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
          initialFilter: 'api_key',
          onConnectTicket() {},
          onEditTicket() {},
          onDeleteTicket() {},
        }),
      ),
    ).not.toThrow();

    expect(() =>
      renderWithTooltip(
        createElement(TicketWalletList, {
          wallet: mixed,
          initialFilter: 'all',
          onConnectTicket() {},
          onEditTicket() {},
          onDeleteTicket() {},
        }),
      ),
    ).not.toThrow();

    const allMarkup = renderWithTooltip(
      createElement(TicketWalletList, {
        wallet: mixed,
        initialFilter: 'all',
        onConnectTicket() {},
        onEditTicket() {},
        onDeleteTicket() {},
      }),
    );
    expect(allMarkup).toContain('Kimi 会员');
    expect(allMarkup).toContain('ChatGPT Plus');
  });
});

describe('TicketDetailPanel', () => {
  it('lays out 用量, 用在哪, footer 导入自, and collapsed 更多 without header duplicates', () => {
    const markup = renderWithTooltip(
      createElement(TicketDetailPanel, {
        id: 'ticket-detail',
        advanced: [
          { label: '协议', value: 'anthropic-messages' },
        ],
        bindings: [{ agent: 'Claude', status: '当前使用' }],
        extras: { quota7dPct: 40, quota7dResetIn: '3d', canEditConfig: true, isCurrent: true },
        importedFromLabel: '导入自 Kimi',
        editLabel: '编辑配置',
        onEdit() {},
        onDelete() {},
      }),
    );
    expect(markup).toContain('id="ticket-detail"');
    expect(markup).toContain('用量');
    expect(markup).toContain('用在哪');
    expect(markup).toContain('导入自 Kimi');
    expect(markup).toContain('更多');
    expect(markup).toContain('<details>');
    expect(markup).not.toContain('<details open');
    expect(markup).toContain('Claude');
    expect(markup).toContain('当前使用');
    expect(markup).not.toContain('正用于');
    expect(markup).not.toContain('Claude · 改配置 · 当前');
    expect(markup).not.toContain('>类型<');
    expect(markup).not.toContain('>来源<');
    expect(markup).not.toContain('>所属<');
    expect(markup).not.toContain('官方账号');
    expect(markup).toContain('编辑配置');
    expect(markup).toContain('移入回收站');
    const moreIndex = markup.indexOf('更多');
    const importIndex = markup.indexOf('导入自 Kimi');
    const protocolIndex = markup.indexOf('anthropic-messages');
    expect(moreIndex).toBeGreaterThan(-1);
    expect(protocolIndex).toBeGreaterThan(moreIndex);
    expect(importIndex).toBeGreaterThan(protocolIndex);
  });

  it('omits 更多 for official login when only import + auth remain', () => {
    const markup = renderWithTooltip(
      createElement(TicketDetailPanel, {
        id: 'oauth-grok-detail',
        advanced: [],
        bindings: [],
        extras: { authLabel: '可续期·未验证' },
        importedFromLabel: '导入自 Grok',
        onDelete() {},
      }),
    );
    expect(markup).toContain('用在哪');
    expect(markup).toContain('导入自 Grok');
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
        bindings: [],
        extras: { authLabel: '可续期·未验证' },
        importedFromLabel: '导入自 Grok',
        onDelete() {},
      }),
    );
    expect(markup).toContain('用在哪');
    expect(markup).toContain('还没接到任何工具');
    expect(markup).toContain('导入自 Grok');
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

describe('TicketWalletList header health chip', () => {
  it('shows a quiet 可续期 chip and never 未验证', () => {
    const markup = renderWithTooltip(
      createElement(TicketWalletList, {
        wallet: sampleWallet(),
        extrasForTicket: () => ({ authLabel: '可续期·未验证' }),
        onConnectTicket() {},
        onEditTicket() {},
        onDeleteTicket() {},
      }),
    );
    expect(markup).toContain('可续期');
    expect(markup).not.toContain('尚未验证');
    expect(markup).not.toContain('未验证');
    expect(markup).not.toContain('可续期·未验证');
  });
});
