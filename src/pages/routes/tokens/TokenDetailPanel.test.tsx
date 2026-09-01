import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { TooltipProvider } from '@/components/ui/tooltip';
import { TokenDetailPanel } from './TokenDetailPanel';
import type { LocalTokenRow } from './tokens-model';

function row(partial: Partial<LocalTokenRow> = {}): LocalTokenRow {
  return {
    id: 'pool-kimi',
    profileId: 'bridge-1',
    name: 'kimi · /v1/chat/completions',
    kind: 'chat_completions',
    path: '/v1/chat/completions',
    endpoint: '127.0.0.1:8123',
    state: 'running',
    token: 'ahb_secret',
    maskedToken: 'ahb_••••cret',
    unavailable: false,
    targetAgentId: 'kimi',
    profileIds: ['bridge-1'],
    lastPath: '/v1/models',
    lastRequestAt: null,
    ...partial,
  };
}

function render(partial: Partial<LocalTokenRow> = {}) {
  return renderToStaticMarkup(
    createElement(
      TooltipProvider,
      null,
      createElement(TokenDetailPanel, {
        row: row(partial),
        onClose: () => {},
      }),
    ),
  );
}

describe('TokenDetailPanel', () => {
  it('shows a test button next to the entry key', () => {
    const markup = render();
    expect(markup).toContain('data-token-detail="pool-kimi"');
    expect(markup).toContain('data-token-test');
    expect(markup).toContain('测试');
    expect(markup).not.toMatch(/data-token-test=""[^>]*\bdisabled\b/);
    expect(markup).not.toContain('ahb_secret');
  });

  it('disables the test button when the entry is not ready', () => {
    const markup = render({ token: null, maskedToken: null, endpoint: null });
    expect(markup).toMatch(/data-token-test=""[^>]*\bdisabled\b/);
    expect(markup).toContain('先填写入口 Key');
  });
});
