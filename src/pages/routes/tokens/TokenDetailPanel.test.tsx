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
    primary: true,
    canDelete: true,
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
    listedModels: ['kimi-k2'],
    lastPath: null,
    lastRequestAt: null,
    usageEligible: true,
    ...partial,
  };
}

function render(
  partial: Partial<LocalTokenRow> = {},
  props: {
    onEditKey?: () => void;
    onDelete?: () => void;
    siblingRows?: LocalTokenRow[];
  } = {},
) {
  const current = row(partial);
  return renderToStaticMarkup(
    createElement(
      TooltipProvider,
      null,
      createElement(TokenDetailPanel, {
        row: current,
        onClose: () => {},
        onEditKey: props.onEditKey,
        onDelete: props.onDelete,
        siblingRows: props.siblingRows ?? [
          current,
          row({ id: 'extra-home', primary: false, name: '家里', canDelete: true }),
        ],
      }),
    ),
  );
}

describe('TokenDetailPanel', () => {
  it('puts test, import, delete, and edit in the inspect header', () => {
    const markup = render({}, { onEditKey: () => {}, onDelete: () => {} });
    expect(markup).toContain('data-token-detail="pool-kimi"');
    expect(markup).toContain('data-token-test');
    expect(markup).toContain('测试');
    expect(markup.indexOf('data-token-test')).toBeLessThan(markup.indexOf('data-token-detail'));
    expect(markup.indexOf('data-token-delete')).toBeLessThan(markup.indexOf('data-token-detail'));
    expect(markup.indexOf('data-token-edit-key')).toBeLessThan(markup.indexOf('data-token-detail'));
    expect(markup).not.toMatch(/data-token-test=""[^>]*\bdisabled\b/);
    expect(markup).not.toMatch(/data-token-delete=""[^>]*\bdisabled\b/);
    expect(markup).not.toContain('ahb_secret');
    expect(markup).toContain('data-token-models');
    expect(markup).toContain('从连接池同步');
    expect(markup).toContain('写进了这些 Agent');
    const body = markup.slice(markup.indexOf('data-token-detail'));
    expect(body.indexOf('入口 Key')).toBeLessThan(body.indexOf('端点'));
    expect(body.indexOf('端点')).toBeLessThan(body.indexOf('类型'));
  });

  it('disables delete when this is the only key of the type', () => {
    const markup = render({}, {
      onDelete: () => {},
      siblingRows: [row()],
    });
    expect(markup).toMatch(/data-token-delete=""[^>]*\bdisabled\b/);
  });

  it('disables the test button when the entry is not ready', () => {
    const markup = render({ token: null, maskedToken: null, endpoint: null });
    expect(markup).toMatch(/data-token-test=""[^>]*\bdisabled\b/);
    expect(markup).toContain('先填写入口 Key');
  });

  it('enables edit key for pool-backed rows and disables it for leftovers', () => {
    const poolMarkup = render({}, { onEditKey: () => {}, onDelete: () => {} });
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
