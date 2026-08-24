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
          isCurrent: true,
          authStatus: 'valid',
          sortKey: '',
        }],
        onImported: vi.fn(),
      })),
    );
    expect(markup).toContain('type="radio"');
    expect(markup).toContain('Work login');
    expect(markup).toContain('cursor-pointer');
    expect(markup).toContain('确认应用');
  });
});

describe('default create name', () => {
  it('uses vendor label plus 备选', () => {
    expect(defaultCreateRouteName('OpenRouter', '备选')).toBe('OpenRouter 备选');
  });
});
