import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { TooltipProvider } from '@/components/ui/tooltip';
import type { RouteTraceFlowRow } from './RouteTraceFlowDiagram';
import { RouteTracePipelineLegend } from './RouteTracePipelineLegend';

const okRow: RouteTraceFlowRow = {
  requestId: 'req-1',
  at: '2026-01-01T00:00:00.000Z',
  method: 'POST',
  path: '/v1/messages',
  httpStatus: 200,
  ok: true,
  localAuth: { status: 'ok' },
  pool: {
    status: 'ok',
    selectedMember: { label: 'Acct A', sourceKind: 'account', sourceId: 'a1' },
  },
  conversion: { status: 'ok', path: 'messages_to_anthropic' },
  upstreamAuth: { status: 'ok' },
  upstream: { status: 'ok', url: 'https://api.anthropic.com/v1/messages' },
};

describe('RouteTracePipelineLegend', () => {
  it('renders five stage cards and keeps supported options on hover', () => {
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
    expect(markup).toContain('data-stage-box="local_auth"');
    expect(markup).toContain('data-stage-box="pool"');
    expect(markup).toContain('data-stage-box="conversion"');
    expect(markup).toContain('data-stage-box="upstream_auth"');
    expect(markup).toContain('data-stage-box="upstream"');
    expect(markup).not.toContain('data-stage-box="local_endpoint"');
    expect(markup).not.toContain('data-stage-catalog');
    expect(markup).toContain('data-card="default"');
    expect(markup).toContain('/v1/messages');
    expect(markup).toContain('/v1/responses');
    expect(markup).toContain('/v1/chat/completions');
    expect(markup).toContain('Acct A');
    expect(markup).toContain('Acct B');
    expect(markup).toContain('https://api.anthropic.com/v1/messages');
    expect(markup).not.toContain('127.0.0.1');
    expect(markup).not.toContain('data-auth-ok');
  });

  it('fills cards with the called option or result', () => {
    const markup = renderToStaticMarkup(
      createElement(
        TooltipProvider,
        null,
        createElement(RouteTracePipelineLegend, {
          row: okRow,
          poolLabels: ['Acct A'],
          upstreamUrls: ['https://api.anthropic.com/v1/messages'],
        }),
      ),
    );
    expect(markup).toContain('data-auth-ok="true"');
    expect(markup).toContain('OK');
    expect(markup).toContain('data-endpoint="messages"');
    expect(markup).toContain('Acct A');
    expect(markup).toContain('Messages');
    expect(markup).toContain('Anthropic');
    expect(markup).toContain('https://api.anthropic.com/v1/messages');
  });
});
