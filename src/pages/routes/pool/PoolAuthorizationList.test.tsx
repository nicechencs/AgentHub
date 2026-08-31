import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { TooltipProvider } from '@/components/ui/tooltip';
import type { PoolAuthorizationItem } from '@/pages/bridges/route-pool-view-model';
import { PoolAuthorizationList } from './PoolAuthorizationList';

function item(partial: Partial<PoolAuthorizationItem> = {}): PoolAuthorizationItem {
  return {
    key: 'account:grok-1',
    sourceKind: 'account',
    sourceId: 'grok-1',
    agentId: 'grok',
    title: 'Grok · OAuth',
    kind: 'oauth',
    surface: 'responses',
    addedHere: true,
    authHealth: 'renewable',
    ...partial,
  };
}

describe('PoolAuthorizationList', () => {
  it('renders one authorization row with login kind and status', () => {
    const markup = renderToStaticMarkup(
      createElement(
        TooltipProvider,
        null,
        createElement(PoolAuthorizationList, { items: [item()] }),
      ),
    );
    expect(markup).toContain('data-pool-authorization="account:grok-1"');
    expect(markup).toContain('Grok · OAuth');
    expect(markup).toContain('官方登录');
    expect(markup).toContain('可续期');
    expect(markup).not.toContain('回复接口');
    expect(markup).not.toContain('本机入口');
    expect(markup).not.toContain('本页添加');
    expect(markup).not.toContain('auth.json');
  });

  it('shows enable switch, last used, quota, bindings, and priority when present', () => {
    const markup = renderToStaticMarkup(
      createElement(
        TooltipProvider,
        null,
        createElement(PoolAuthorizationList, {
          items: [item({
            canToggle: true,
            enabled: true,
            priority: 1,
            lastUsedAt: '2026-08-31T08:00:00Z',
            quota7dPct: 22,
            bindingCount: 1,
          })],
        }),
      ),
    );
    expect(markup).toContain('role="switch"');
    expect(markup).toContain('7d 22%');
    expect(markup).toContain('1 个连接');
    expect(markup).toContain('优先级 1');
  });

  it('marks the row as openable when a detail handler is provided', () => {
    const markup = renderToStaticMarkup(
      createElement(
        TooltipProvider,
        null,
        createElement(PoolAuthorizationList, {
          items: [item()],
          activeKey: 'account:grok-1',
          onShowDetail: () => {},
        }),
      ),
    );
    expect(markup).toContain('data-active="true"');
    expect(markup).toContain('tabindex="0"');
    expect(markup).toContain('cursor-pointer');
  });
});
