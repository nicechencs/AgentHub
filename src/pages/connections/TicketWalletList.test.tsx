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
    expect(markup).not.toContain('移入回收站');
    expect(markup).not.toContain('编辑配置');
    expect(markup).not.toContain('编辑 API Key');
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
});

describe('TicketDetailPanel', () => {
  it('renders ticket fields, bindings, and secondary edit/delete', () => {
    const markup = renderWithTooltip(
      createElement(TicketDetailPanel, {
        id: 'ticket-detail',
        fields: [
          { label: '类型', value: 'API Key' },
          { label: '票面', value: '会员' },
        ],
        bindingLines: ['Claude · 改配置 · 当前'],
        extras: { canEditConfig: true, isCurrent: true },
        editLabel: '编辑配置',
        onEdit() {},
        onDelete() {},
      }),
    );
    expect(markup).toContain('id="ticket-detail"');
    expect(markup).toContain('类型');
    expect(markup).toContain('API Key');
    expect(markup).toContain('正用于');
    expect(markup).toContain('Claude · 改配置 · 当前');
    expect(markup).toContain('编辑配置');
    expect(markup).toContain('移入回收站');
  });

  it('does not offer edit for OAuth tickets', () => {
    const markup = renderWithTooltip(
      createElement(TicketDetailPanel, {
        id: 'oauth-detail',
        fields: [{ label: '类型', value: '官方登录' }],
        bindingLines: [],
        extras: { authLabel: '可续期·未验证' },
        onDelete() {},
      }),
    );
    expect(markup).toContain('官方登录');
    expect(markup).toContain('未绑定任何 Agent');
    expect(markup).toContain('可续期·未验证');
    expect(markup).not.toContain('编辑密钥');
    expect(markup).not.toContain('编辑配置');
  });
});
