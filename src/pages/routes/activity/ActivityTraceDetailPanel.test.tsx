import { createElement, type ReactElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { TooltipProvider } from '@/components/ui/tooltip';
import type { RouteTraceListItem } from '@/components/shared/RouteTraceList';
import { ActivityTraceDetailPanel } from './ActivityTraceDetailPanel';

function row(partial: Partial<RouteTraceListItem> = {}): RouteTraceListItem {
  return {
    requestId: 'req-1',
    at: '2026-01-01T00:00:00.000Z',
    method: 'POST',
    path: '/v1/messages',
    httpStatus: 200,
    ok: true,
    model: 'claude-sonnet',
    latencyMs: 4200,
    ttftMs: 800,
    inputTokens: 1200,
    outputTokens: 340,
    localAuth: { status: 'ok', port: 8787 },
    pool: { status: 'ok', selectedMember: { label: 'acct-1', sourceKind: 'account', sourceId: 'acct-1' } },
    conversion: { status: 'ok', path: 'messages_to_anthropic', result: 'converted' },
    upstreamAuth: { status: 'ok', httpStatus: 200 },
    upstream: {
      status: 'ok',
      url: 'https://api.anthropic.com/v1/messages',
      httpStatus: 200,
    },
    sourceLabel: 'Route A',
    ...partial,
  };
}

function render(node: ReactElement): string {
  return renderToStaticMarkup(createElement(TooltipProvider, null, node));
}

describe('ActivityTraceDetailPanel', () => {
  it('shows inbound and outbound endpoints plus five-stage results', () => {
    const markup = render(
      createElement(ActivityTraceDetailPanel, {
        row: row(),
        width: 360,
        onClose() {},
      }),
    );
    expect(markup).toContain('data-side-inspect');
    expect(markup).toContain('data-activity-trace-detail="req-1"');
    expect(markup).toContain('请求详情');
    expect(markup).toContain('本地调用端点');
    expect(markup).toContain('/v1/messages');
    expect(markup).toContain('上游调用端点');
    expect(markup).toContain('https://api.anthropic.com/v1/messages');
    expect(markup).toContain('本机鉴权');
    expect(markup).toContain('连接池');
    expect(markup).toContain('转换');
    expect(markup).toContain('上游鉴权');
    expect(markup).toContain('上游');
    expect(markup).toContain('data-stage="local_auth"');
    expect(markup).toContain('data-stage-status="ok"');
  });
});
