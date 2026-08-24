import { createElement, type ReactNode } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { TooltipProvider } from '@/components/ui/tooltip';
import { CreateRouteDialog } from './CreateRouteDialog';
import { ImportRouteDialog } from './ImportRouteDialog';
import { defaultCreateRouteName, endpointUrlFor, vendorById } from './create-route-flow';

vi.mock('@/components/ui/dialog', () => {
  const passthrough = ({ children }: { children?: ReactNode }) => children ?? null;
  return {
    Dialog: ({ open, children }: { open?: boolean; children?: ReactNode }) =>
      (open ? children : null),
    DialogContent: passthrough,
    DialogHeader: passthrough,
    DialogFooter: passthrough,
    DialogTitle: passthrough,
    DialogDescription: passthrough,
  };
});

vi.mock('react-router-dom', () => ({
  Link: ({ to, children }: { to: string; children?: ReactNode }) =>
    createElement('a', { href: to }, children),
}));

function renderCreate() {
  return renderToStaticMarkup(
    createElement(TooltipProvider, null, createElement(CreateRouteDialog, {
      open: true,
      onOpenChange: vi.fn(),
      onCreated: vi.fn(),
    })),
  );
}

describe('CreateRouteDialog', () => {
  it('disables 确认应用 until name+url+key+endpoint are filled', () => {
    const markup = renderCreate();
    expect(markup).toContain('确认应用');
    expect(markup).toContain('disabled');
    expect(markup).toContain('接到');
    expect(markup).toContain(endpointUrlFor('openrouter', 'claude', vendorById('openrouter').url));
    expect(markup).toContain('type="submit"');
  });

  it('keeps SecretInput and does not invent a second URL field', () => {
    const markup = renderCreate();
    expect(markup).toContain('type="password"');
    expect(markup).not.toContain('票');
    expect(markup).not.toContain('钱包');
    expect(markup).not.toContain('投影');
    expect(markup).not.toContain('协议桥');
  });
});

describe('ImportRouteDialog', () => {
  it('points empty state to 连接', () => {
    const markup = renderToStaticMarkup(
      createElement(TooltipProvider, null, createElement(ImportRouteDialog, {
        open: true,
        onOpenChange: vi.fn(),
        entries: [],
        onImported: vi.fn(),
      })),
    );
    expect(markup).toContain('href="/connections"');
    expect(markup).toContain('连接');
    expect(markup).toContain('用这份登录');
    expect(markup).toContain('disabled');
  });

  it('renders each login as a row-sized radio label', () => {
    const markup = renderToStaticMarkup(
      createElement(TooltipProvider, null, createElement(ImportRouteDialog, {
        open: true,
        onOpenChange: vi.fn(),
        entries: [{
          key: 'account:acc-1',
          source: 'account',
          kind: 'apikey',
          id: 'acc-1',
          agentId: 'claude',
          title: 'Work login',
          subtitle: '已配置',
          endpointMode: 'official',
          isCurrent: true,
          authStatus: 'valid',
          sortKey: '',
        }],
        onImported: vi.fn(),
      })),
    );
    expect(markup).toContain('type="radio"');
    expect(markup).toContain('Work login · Claude · 官方端点');
    expect(markup).toContain('cursor-pointer');
    expect(markup).toContain('确认应用');
  });

  it('omits generated 本机路由 logins from import', () => {
    const markup = renderToStaticMarkup(
      createElement(TooltipProvider, null, createElement(ImportRouteDialog, {
        open: true,
        onOpenChange: vi.fn(),
        entries: [
          {
            key: 'provider:p-1',
            source: 'provider',
            kind: 'apikey',
            id: 'p-1',
            agentId: 'claude',
            title: '本机路由',
            subtitle: '已配置 · 官方端点',
            isCurrent: true,
            authStatus: 'valid',
            sortKey: '',
            endpointMode: 'official',
          },
          {
            key: 'provider:p-2',
            source: 'provider',
            kind: 'apikey',
            id: 'p-2',
            agentId: 'codex',
            title: '本机路由',
            subtitle: '已配置 · 自定义端点',
            isCurrent: false,
            authStatus: 'valid',
            sortKey: '',
            endpointMode: 'custom',
          },
        ],
        onImported: vi.fn(),
      })),
    );
    expect(markup).not.toContain('本机路由 · Claude · 官方端点');
    expect(markup).not.toContain('本机路由 · Codex · 自定义端点');
    expect(markup).toContain('还没有可导入的登录');
    expect(markup).not.toContain('票');
    expect(markup).not.toContain('钱包');
    expect(markup).not.toContain('投影');
    expect(markup).not.toContain('协议桥');
    expect(markup).not.toContain('PKCE');
  });

  it('omits logins already attached to a local-bridge profile', () => {
    const markup = renderToStaticMarkup(
      createElement(TooltipProvider, null, createElement(ImportRouteDialog, {
        open: true,
        onOpenChange: vi.fn(),
        entries: [
          {
            key: 'account:acc-1',
            source: 'account',
            kind: 'apikey',
            id: 'acc-1',
            agentId: 'claude',
            title: 'Already routed',
            subtitle: '已配置',
            endpointMode: 'official',
            isCurrent: true,
            authStatus: 'valid',
            sortKey: '',
          },
          {
            key: 'account:acc-2',
            source: 'account',
            kind: 'apikey',
            id: 'acc-2',
            agentId: 'codex',
            title: 'Still free',
            subtitle: '已配置',
            endpointMode: 'official',
            isCurrent: false,
            authStatus: 'valid',
            sortKey: '',
          },
        ],
        profiles: [
          { id: 'prof-1', sourceKind: 'account', sourceId: 'acc-1', route: 'local_bridge' },
        ],
        onImported: vi.fn(),
      })),
    );
    expect(markup).not.toContain('Already routed');
    expect(markup).toContain('Still free · Codex · 官方端点');
    expect(markup).toContain('type="radio"');
  });
});

describe('default create name', () => {
  it('uses vendor label plus 备选', () => {
    expect(defaultCreateRouteName('OpenRouter', '备选')).toBe('OpenRouter 备选');
  });
});
