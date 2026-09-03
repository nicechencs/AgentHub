import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { TooltipProvider } from '@/components/ui/tooltip';
import type { PoolAuthorizationItem } from '@/pages/routes/shared/route-pool-view-model';
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
    endpointKinds: ['responses_grok'],
    addedHere: true,
    authHealth: 'renewable',
    ...partial,
  };
}

describe('PoolAuthorizationList', () => {
  it('renders a field table with login kind and status', () => {
    const markup = renderToStaticMarkup(
      createElement(
        TooltipProvider,
        null,
        createElement(PoolAuthorizationList, { items: [item()] }),
      ),
    );
    expect(markup).toContain('<table');
    expect(markup).toContain('data-col="enabled"');
    expect(markup).toContain('data-col="login"');
    expect(markup).toContain('data-col="kind"');
    expect(markup).toContain('data-col="endpointTypes"');
    expect(markup).toContain('data-col="status"');
    expect(markup).toContain('/v1/responses');
    expect(markup).toContain('var(--agent-grok)');
    expect(markup).toContain('data-pool-authorization="account:grok-1"');
    expect(markup).toContain('Grok · OAuth');
    expect(markup).toContain('data-pool-login-mark="oauth"');
    expect(markup).toContain('官方登录');
    expect(markup).toContain('可续期');
    expect(markup).not.toContain('data-col="bindings"');
    expect(markup).not.toContain('data-col="quota"');
    expect(markup).not.toContain('data-col="lastUsed"');
    expect(markup).not.toContain('data-col="priority"');
    expect(markup).toContain('data-table-shell="default"');
    expect(markup).toContain('data-table-layout="split"');
    expect(markup).toContain('role="separator"');
    expect(markup).toContain('调整登录列宽');
    expect(markup).not.toContain('回复接口');
    expect(markup).not.toContain('本机转发');
    expect(markup).not.toContain('本页添加');
    expect(markup).not.toContain('auth.json');
  });

  it('adds enable, quota, bindings, last used, and priority columns when present', () => {
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
    expect(markup).toContain('data-col="bindings"');
    expect(markup).toContain('data-col="quota"');
    expect(markup).toContain('data-col="lastUsed"');
    expect(markup).toContain('data-col="priority"');
    expect(markup).toContain('7d 22%');
    expect(markup).toContain('连接数量');
    expect(markup).toContain('调用窗口');
    expect(markup).toContain('最近使用');
    expect(markup).toContain('优先级');
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

  it('shows a custom login as domain and wraps extra endpoint types', () => {
    const markup = renderToStaticMarkup(
      createElement(
        TooltipProvider,
        null,
        createElement(PoolAuthorizationList, {
          items: [item({
            key: 'provider:or-1',
            sourceKind: 'provider',
            sourceId: 'or-1',
            agentId: 'claude',
            title: 'OpenRouter · openrouter.ai/api/v1',
            kind: 'apikey',
            endpointMode: 'custom',
            endpointHost: 'openrouter.ai/api/v1',
            endpointKinds: ['messages', 'chat_completions'],
          })],
        }),
      ),
    );
    expect(markup).toContain('openrouter.ai');
    expect(markup).not.toContain('OpenRouter · openrouter.ai/api/v1');
    expect(markup).not.toContain('Claude Code');
    expect(markup).toContain('data-pool-login-mark="url"');
    expect(markup).toContain('linearGradient');
    expect(markup).toContain('var(--agent-claude)');
    expect(markup).toContain('var(--agent-codex)');
    expect(markup).toContain('/v1/messages');
    expect(markup).toContain('/v1/chat/completions');
    expect(markup).toContain('flex-col');
  });
});
