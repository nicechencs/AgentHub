import { createElement, type ReactElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { TooltipProvider } from '@/components/ui/tooltip';
import type { RouteTraceListItem } from '@/components/shared/RouteTraceList';
import { ActivityTraceDetailPanel } from './ActivityTraceDetailPanel';

function row(partial: Partial<RouteTraceListItem> = {}): RouteTraceListItem {
  return {
    traceVersion: 2,
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
    localEndpoint: { status: 'ok' },
    localAuth: { status: 'ok', port: 8787, keyLast4: 'local' },
    admission: { status: 'ok' },
    routeResolution: { status: 'ok' },
    pool: { status: 'ok', selectedMember: { label: 'WorkBuddy Grok', sourceKind: 'account', sourceId: 'acct-1', keyLast4: '627a' } },
    conversion: { status: 'ok', path: 'messages_to_anthropic', result: 'converted' },
    upstreamAuth: { status: 'ok', httpStatus: 200 },
    upstreamRequest: {
      status: 'ok',
      url: 'https://api.anthropic.com/v1/messages',
      member: { label: 'WorkBuddy Grok', sourceKind: 'account', sourceId: 'acct-1', keyLast4: '627a' },
      model: 'claude-sonnet-upstream',
    },
    upstream: {
      status: 'ok',
      url: 'https://api.anthropic.com/v1/messages',
      upstreamModel: 'claude-sonnet-upstream',
      httpStatus: 200,
    },
    responseConversion: { status: 'ok', path: 'anthropic_to_messages', result: 'completed' },
    delivery: { status: 'ok', httpStatus: 200, stream: false, completion: 'response_returned' },
    sourceLabel: 'Route A',
    ...partial,
  };
}

function render(node: ReactElement): string {
  return renderToStaticMarkup(createElement(TooltipProvider, null, node));
}

describe('ActivityTraceDetailPanel', () => {
  it('shows the eleven-node request chain as expandable status cards', () => {
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
    expect(markup).toContain('本次请求实际走向');
    expect(markup).toContain('点开每个节点可查看具体数据');
    for (const stage of [
      'received', 'local_auth', 'local_endpoint', 'admission', 'route_resolution', 'pool',
      'request_conversion', 'upstream_request', 'upstream_response', 'response_conversion', 'delivery',
    ]) {
      expect(markup).toContain(`data-detail-stage="${stage}"`);
    }
    expect(markup.match(/data-detail-stage=/g)).toHaveLength(11);
    expect(markup).toContain('aria-expanded="true"');
    expect(markup).toContain('请求 ID');
    expect(markup).toContain('req-1');
    expect(markup).toContain('••••local');
    expect(markup).not.toContain('sk-deepseek');
  });

  it('shows a compact failure summary and marks later stages as not reached', () => {
    const markup = render(
      createElement(ActivityTraceDetailPanel, {
        row: row({
          ok: false,
          httpStatus: 401,
          upstreamAuth: { status: 'failed', httpStatus: 401, code: 'unauthorized' },
          upstream: { status: 'pending' },
          responseConversion: { status: 'skipped', path: '' },
          delivery: { status: 'ok', httpStatus: 401, stream: false, completion: 'response_returned' },
          failureStage: 'upstream_response',
        }),
        width: 360,
        onClose() {},
      }),
    );
    expect(markup).toContain('>失败</span>');
    expect(markup).toContain('接收上游响应失败');
    expect(markup).toContain('>401</p>');
    expect(markup).toContain('unauthorized');
    expect(markup).toContain('data-detail-stage="upstream_response"');
    expect(markup).toContain('data-stage-status="failed"');
    expect(markup).toContain('data-detail-stage="response_conversion"');
    expect(markup).toContain('data-stage-status="skipped"');
    expect(markup).toContain('未到达');
  });
});
