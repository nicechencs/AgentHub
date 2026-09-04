import { createElement, type ReactElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { TooltipProvider } from '@/components/ui/tooltip';
import type { RouteTraceListItem } from '@/components/shared/RouteTraceList';
import { ActivityTraceList } from './ActivityTraceList';
import { ACTIVITY_TRACE_WIDTH_SPECS } from './activity-trace-list-model';

function row(partial: Partial<RouteTraceListItem> = {}): RouteTraceListItem {
  return {
    traceVersion: 2,
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
    localAuth: { status: 'failed', code: 'invalid_api_key', keyLast4: '1234', port: 8787 },
    pool: { status: 'skipped' },
    conversion: { status: 'skipped', path: '' },
    upstreamAuth: { status: 'skipped' },
    upstream: { status: 'skipped', url: 'https://api.anthropic.com/v1/messages' },
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
    expect(markup).toContain('data-col="key"');
    expect(markup).toContain('data-col="endpoint"');
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
    expect(markup).toContain('••••1234');
    expect(markup).toContain('/v1/messages');
    expect(markup).toContain('入站');
    expect(markup).toContain('出站');
    expect(markup).toContain('data-endpoint-dir="in"');
    expect(markup).toContain('data-endpoint-dir="out"');
    expect(markup).toContain('aria-label="入站 · 本地调用端点 http://127.0.0.1:8787/v1/messages"');
    expect(markup).toContain('aria-label="出站 · 最终上游调用端点 https://api.anthropic.com/v1/messages"');
    expect(markup).toContain('aria-label="本机鉴权入口 Key ••••1234"');
    expect(markup).toContain('claude-sonnet');
    expect(markup).toContain('0.8s');
    expect(markup).toContain('4.2s');
    expect(markup).toContain('1.2K / 340');
    expect(markup).toContain('Route A');
    expect(markup).toContain('详情');
    expect(markup).toContain('失败于 本机鉴权');
    expect(markup).toContain('data-activity-trace-result="failed"');
    expect(markup).toContain('min-w-max');
    expect(markup).toContain('data-stage="local_auth"');
    expect(markup).toContain('data-stage-status="failed"');
    expect(markup).toContain('data-table-layout="split"');
  });

  it('shows success in the existing five-stage cell', () => {
    const markup = render(createElement(ActivityTraceList, {
      rows: [row({ ok: true, httpStatus: 200, localAuth: { status: 'ok' }, failureStage: null })],
    }));
    expect(markup).toContain('>成功</span>');
    expect(markup).toContain('data-activity-trace-result="success"');
  });

  it('reserves one line for the result text and all five stage icons', () => {
    expect(ACTIVITY_TRACE_WIDTH_SPECS.slice(0, 8).map(({ key, defaultWidth }) => ({ key, defaultWidth }))).toEqual([
      { key: 'time', defaultWidth: 148 },
      { key: 'key', defaultWidth: 168 },
      { key: 'endpoint', defaultWidth: 236 },
      { key: 'model', defaultWidth: 120 },
      { key: 'firstToken', defaultWidth: 72 },
      { key: 'duration', defaultWidth: 88 },
      { key: 'tokens', defaultWidth: 104 },
      { key: 'stages', defaultWidth: 224 },
    ]);
    const stages = ACTIVITY_TRACE_WIDTH_SPECS.find((spec) => spec.key === 'stages');
    expect(stages).toMatchObject({ defaultWidth: 224, minWidth: 196 });
  });

  it('renders the key abbreviation with its name', () => {
    const markup = render(createElement(ActivityTraceList, {
      rows: [row()],
      tokens: [{ token: 'ahb_local_1234', name: 'Claude 入口', poolId: 'pool-a' }],
    }));
    expect(markup).toContain('ahb_••••1234');
    expect(markup).toContain('Claude 入口');
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
