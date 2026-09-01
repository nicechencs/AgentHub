import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { TooltipProvider } from '@/components/ui/tooltip';
import type { PoolAuthorizationItem } from '@/pages/bridges/route-pool-view-model';
import { PoolAuthorizationDetail } from './PoolAuthorizationDetail';

function item(partial: Partial<PoolAuthorizationItem> = {}): PoolAuthorizationItem {
  return {
    key: 'account:grok-1',
    sourceKind: 'account',
    sourceId: 'grok-1',
    agentId: 'grok',
    title: 'user@x.ai',
    kind: 'oauth',
    surface: 'responses',
    endpointKinds: ['responses_grok'],
    addedHere: true,
    canToggle: true,
    enabled: true,
    priority: 0,
    lastUsedAt: '2026-08-31T12:04:00Z',
    quota7dPct: 41,
    bindingCount: 2,
    ...partial,
  };
}

describe('PoolAuthorizationDetail', () => {
  it('shows account fields without login-file cards', () => {
    const markup = renderToStaticMarkup(
      createElement(
        TooltipProvider,
        null,
        createElement(PoolAuthorizationDetail, {
          item: item(),
          oauthAction: { kind: 'refresh-quota', label: '刷新' },
          onRefresh() {},
          onDelete() {},
          onClose() {},
        }),
      ),
    );
    expect(markup).toContain('data-pool-authorization-detail="account:grok-1"');
    expect(markup).toContain('登录详情');
    expect(markup).toContain('刷新');
    expect(markup).toContain('启用');
    expect(markup).toContain('调用窗口');
    expect(markup).toContain('7d');
    expect(markup).toContain('2 个连接');
    expect(markup).toContain('优先级');
    expect(markup).toContain('最近使用');
    expect(markup).not.toContain('相关文件');
    expect(markup).not.toContain('auth.json');
    expect(markup).not.toContain('config.toml');
  });

  it('hides refresh when the login has no refresh action', () => {
    const markup = renderToStaticMarkup(
      createElement(
        TooltipProvider,
        null,
        createElement(PoolAuthorizationDetail, {
          item: item({ kind: 'apikey' }),
          onDelete() {},
          onClose() {},
        }),
      ),
    );
    expect(markup).not.toContain('刷新');
    expect(markup).toContain('移入回收站');
  });

  it('shows 刷新中… while a refresh is in flight', () => {
    const markup = renderToStaticMarkup(
      createElement(
        TooltipProvider,
        null,
        createElement(PoolAuthorizationDetail, {
          item: item(),
          oauthAction: { kind: 'refresh-quota', label: '刷新' },
          refreshing: true,
          onRefresh() {},
          onDelete() {},
          onClose() {},
        }),
      ),
    );
    expect(markup).toContain('刷新中…');
  });
});
