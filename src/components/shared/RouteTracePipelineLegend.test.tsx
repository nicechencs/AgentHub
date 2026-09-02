import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { TooltipProvider } from '@/components/ui/tooltip';
import { RouteTracePipelineLegend } from './RouteTracePipelineLegend';

describe('RouteTracePipelineLegend', () => {
  it('shows four local endpoints, pool logins, and upstream urls without extra chrome', () => {
    const markup = renderToStaticMarkup(
      createElement(
        TooltipProvider,
        null,
        createElement(RouteTracePipelineLegend, {
          poolLabels: ['Acct A', 'Acct B'],
          upstreamUrls: ['https://api.anthropic.com/v1/messages'],
        }),
      ),
    );
    expect(markup).toContain('data-endpoint="messages"');
    expect(markup).toContain('data-endpoint="responses_codex"');
    expect(markup).toContain('data-endpoint="responses_grok"');
    expect(markup).toContain('data-endpoint="chat_completions"');
    expect(markup).toContain('/v1/messages');
    expect(markup).toContain('/v1/responses');
    expect(markup).toContain('/v1/chat/completions');
    expect(markup).toContain('Acct A');
    expect(markup).toContain('Acct B');
    expect(markup).toContain('https://api.anthropic.com/v1/messages');
    expect(markup).not.toContain('127.0.0.1');
    expect(markup).not.toContain('data-matrix-cell');
    expect(markup).not.toContain('入口 Key 验证通过');
  });
});
