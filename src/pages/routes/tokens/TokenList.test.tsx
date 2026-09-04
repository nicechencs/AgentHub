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
    primary: true,
    canDelete: false,
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
    listedModels: [],
    ...partial,
  };
}

describe('TokenList', () => {
  it('groups keys by type and puts the endpoint on the group header', () => {
    const markup = renderToStaticMarkup(
      createElement(
        TooltipProvider,
        null,
        createElement(TokenList, { rows: [row()] }),
      ),
    );
    expect(markup).toContain('<table');
    expect(markup).toContain('data-col="name"');
    expect(markup).toContain('data-col="token"');
    expect(markup).not.toContain('data-col="type"');
    expect(markup).not.toContain('data-col="endpoint"');
    expect(markup).not.toContain('data-col="lastPage"');
    expect(markup).not.toContain('data-col="usage"');
    expect(markup).toContain('overflow-hidden');
    expect(markup).toContain('truncate');
    expect(markup).toContain('data-token-group="chat_completions"');
    expect(markup).toContain('data-token-row="pool-kimi"');
    expect(markup).toContain('Chat Completions');
    expect(markup).toContain('http://127.0.0.1:8123');
    expect(markup).toContain('/v1/chat/completions');
    expect(markup).toContain('ahb_••••cret');
    expect(markup).not.toContain('1.2K in / 800 out');
    expect(markup).not.toContain('ahb_secret');
    expect(markup).not.toContain('修改');
    expect(markup).toContain('data-table-shell="default"');
    expect(markup).toContain('data-table-layout="split"');
    expect(markup).toContain('role="separator"');
    expect(markup).toContain('调整名称列宽');
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

  it('labels an unnamed default key as 默认', () => {
    const markup = renderToStaticMarkup(
      createElement(
        TooltipProvider,
        null,
        createElement(TokenList, {
          rows: [row({ name: '', primary: true })],
        }),
      ),
    );
    expect(markup).toContain('默认');
  });
});
