import { createElement, type ReactElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { TooltipProvider } from '@/components/ui/tooltip';
import type { RouteTraceListItem } from '@/components/shared/RouteTraceList';
import { ActivityTraceList } from './ActivityTraceList';

function row(partial: Partial<RouteTraceListItem> = {}): RouteTraceListItem {
  return {
    requestId: 'req-1',
    at: '2026-01-01T00:00:00.000Z',
    method: 'POST',
    path: '/v1/messages',
    httpStatus: 401,
    ok: false,
    model: 'claude-sonnet',
    latencyMs: 4200,
    ttftMs: 800,
    inputTokens: 1200,
    outputTokens: 340,
    localAuth: { status: 'failed', code: 'invalid_api_key' },
    pool: { status: 'skipped' },
    conversion: { status: 'skipped', path: '' },
    upstreamAuth: { status: 'skipped' },
    upstream: { status: 'skipped' },
    failureStage: 'local_auth',
    sourceLabel: 'Route A',
    ...partial,
  };
}

function render(node: ReactElement): string {
  return renderToStaticMarkup(createElement(TooltipProvider, null, node));
}

describe('ActivityTraceList', () => {
  it('renders monitoring fields without a result column or inline expand', () => {
    const markup = render(createElement(ActivityTraceList, { rows: [row()] }));
    expect(markup).toContain('<table');
    expect(markup).toContain('data-col="time"');
    expect(markup).toContain('data-col="request"');
    expect(markup).toContain('data-col="model"');
    expect(markup).toContain('data-col="firstToken"');
    expect(markup).toContain('data-col="duration"');
    expect(markup).toContain('data-col="tokens"');
    expect(markup).toContain('data-col="stages"');
    expect(markup).toContain('data-col="route"');
    expect(markup).toContain('data-col="details"');
    expect(markup).not.toContain('data-col="result"');
    expect(markup).not.toContain('data-col="latency"');
    expect(markup).not.toContain('data-activity-trace-detail');
    expect(markup).toContain('data-activity-trace-row="req-1"');
    expect(markup).toContain('2026-01-01 00:00:00');
    expect(markup).toContain('POST');
    expect(markup).toContain('/v1/messages');
    expect(markup).toContain('claude-sonnet');
    expect(markup).toContain('0.8s');
    expect(markup).toContain('4.2s');
    expect(markup).toContain('1.2K / 340');
    expect(markup).toContain('Route A');
    expect(markup).toContain('详情');
    expect(markup).toContain('data-stage="local_auth"');
    expect(markup).toContain('data-stage-status="failed"');
    expect(markup).toContain('data-table-layout="split"');
  });

  it('shows the empty label in the table body when there are no rows', () => {
    const markup = render(
      createElement(ActivityTraceList, { rows: [], emptyLabel: '还没有请求记录' }),
    );
    expect(markup).toContain('<table');
    expect(markup).toContain('还没有请求记录');
    expect(markup).not.toContain('data-activity-trace-row');
  });
});
