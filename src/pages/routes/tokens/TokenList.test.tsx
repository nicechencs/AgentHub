import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { TooltipProvider } from '@/components/ui/tooltip';
import { TokenList } from './TokenList';
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
    ...partial,
  };
}

describe('TokenList', () => {
  it('renders a field table with type, endpoint, and token', () => {
    const markup = renderToStaticMarkup(
      createElement(
        TooltipProvider,
        null,
        createElement(TokenList, { rows: [row()], onEditKey: () => {} }),
      ),
    );
    expect(markup).toContain('<table');
    expect(markup).toContain('data-col="type"');
    expect(markup).toContain('data-col="endpoint"');
    expect(markup).toContain('data-col="token"');
    expect(markup).toContain('data-token-row="pool-kimi"');
    expect(markup).toContain('Chat Completions');
    expect(markup).toContain('http://127.0.0.1:8123');
    expect(markup).toContain('/v1/chat/completions');
    expect(markup).toContain('ahb_••••cret');
    expect(markup).not.toContain('ahb_secret');
    expect(markup).toContain('修改');
    expect(markup).toContain('data-table-shell="default"');
    expect(markup).toContain('role="separator"');
    expect(markup).toContain('调整类型列宽');
  });

  it('shows a dash when the entry key is not ready', () => {
    const markup = renderToStaticMarkup(
      createElement(
        TooltipProvider,
        null,
        createElement(TokenList, {
          rows: [row({ token: null, maskedToken: null })],
        }),
      ),
    );
    expect(markup).toContain('—');
    expect(markup).not.toContain('ahb_');
  });
});
