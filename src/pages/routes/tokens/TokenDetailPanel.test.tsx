import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { TooltipProvider } from '@/components/ui/tooltip';
import { TokenDetailPanel } from './TokenDetailPanel';
import type { LocalTokenRow } from './tokens-model';

function row(partial: Partial<LocalTokenRow> = {}): LocalTokenRow {
  return {
    id: 'pool-kimi',
    poolBacked: true,
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
    listedModels: ['kimi-k2'],
    ...partial,
  };
}

function render(
  partial: Partial<LocalTokenRow> = {},
  props: { onEditKey?: () => void } = {},
) {
  return renderToStaticMarkup(
    createElement(
      TooltipProvider,
      null,
      createElement(TokenDetailPanel, {
        row: row(partial),
        onClose: () => {},
        onEditKey: props.onEditKey,
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
    expect(markup).toContain('data-token-models');
    expect(markup).toContain('按连接池更新');
    expect(markup).toContain('kimi-k2');
  });

  it('disables the test button when the entry is not ready', () => {
    const markup = render({ token: null, maskedToken: null, endpoint: null });
    expect(markup).toMatch(/data-token-test=""[^>]*\bdisabled\b/);
    expect(markup).toContain('先填写入口 Key');
  });

  it('enables edit key for pool-backed rows and disables it for leftovers', () => {
    const poolMarkup = render({}, { onEditKey: () => {} });
    expect(poolMarkup).toContain('data-token-edit-key');
    expect(poolMarkup).not.toMatch(/data-token-edit-key=""[^>]*\bdisabled\b/);

    const leftoverMarkup = render(
      { id: 'orphan-bridge', poolBacked: false, profileId: 'orphan-bridge' },
      { onEditKey: () => {} },
    );
    expect(leftoverMarkup).toMatch(/data-token-edit-key=""[^>]*\bdisabled\b/);
    expect(leftoverMarkup).toContain('这条还不是连接池入口 Key');
  });

});
