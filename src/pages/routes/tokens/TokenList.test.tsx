import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { TooltipProvider } from '@/components/ui/tooltip';
import { TokenList } from './TokenList';
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
    lastPath: '/v1/chat/completions',
    lastRequestAt: '2026-08-31T12:00:00.000Z',
    usage: {
      requestCount: 4,
      inputTokens: 1200,
      outputTokens: 800,
      cachedInputTokens: 0,
    },
    listedModels: [],
    ...partial,
  };
}

describe('TokenList', () => {
  it('renders a field table with type, endpoint, token, last page, and usage', () => {
    const markup = renderToStaticMarkup(
      createElement(
        TooltipProvider,
        null,
        createElement(TokenList, { rows: [row()] }),
      ),
    );
    expect(markup).toContain('<table');
    expect(markup).toContain('data-col="type"');
    expect(markup).toContain('data-col="endpoint"');
    expect(markup).toContain('overflow-hidden');
    expect(markup).toContain('truncate');
    expect(markup).toContain('data-col="token"');
    expect(markup).toContain('data-col="lastPage"');
    expect(markup).toContain('data-col="usage"');
    expect(markup).toContain('data-token-row="pool-kimi"');
    expect(markup).toContain('Chat Completions');
    expect(markup).toContain('http://127.0.0.1:8123');
    expect(markup).toContain('/v1/chat/completions');
    expect(markup).toContain('ahb_••••cret');
    expect(markup).toContain('1.2K in / 800 out');
    expect(markup).not.toContain('ahb_secret');
    expect(markup).not.toContain('修改');
    expect(markup).toContain('data-table-shell="default"');
    expect(markup).toContain('data-table-layout="split"');
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

  it('shows dashes when last page and usage are empty', () => {
    const markup = renderToStaticMarkup(
      createElement(
        TooltipProvider,
        null,
        createElement(TokenList, {
          rows: [row({
            lastPath: null,
            lastRequestAt: null,
            usage: { requestCount: 0, inputTokens: 0, outputTokens: 0, cachedInputTokens: 0 },
          })],
        }),
      ),
    );
    expect(markup).toContain('data-col="lastPage"');
    expect(markup).toContain('data-col="usage"');
    expect(markup).not.toContain('in /');
  });
});
