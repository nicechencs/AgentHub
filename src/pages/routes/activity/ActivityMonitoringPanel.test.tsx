import { createElement, type ReactElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { MemoryRouter } from 'react-router-dom';
import { describe, expect, it, vi } from 'vitest';
import { TooltipProvider } from '@/components/ui/tooltip';
import { ActivityMonitoringPanel } from './ActivityMonitoringPanel';
import type { ActivityPageSnapshot } from './activity-view-model';
import type { MergedRouteTraceRow } from './route-trace-feed-model';

vi.mock('@/components/shared/RouteTracePipelineLegend', () => ({
  RouteTracePipelineLegend: ({ row }: { row?: { requestId: string } }) => createElement('div', {
    'data-route-trace-legend-row': row?.requestId ?? 'fallback',
  }),
}));

function render(node: ReactElement): string {
  return renderToStaticMarkup(
    createElement(MemoryRouter, null, createElement(TooltipProvider, null, node)),
  );
}

function snapshot(partial: Partial<ActivityPageSnapshot> = {}): ActivityPageSnapshot {
  return {
    kind: 'noRoutes',
    feed: [],
    monitoredProfileIds: [],
    runningCount: 0,
    hasEnrolledLogins: true,
    allCount: 0,
    failedCount: 0,
    ...partial,
  };
}

function row(partial: Partial<MergedRouteTraceRow> = {}): MergedRouteTraceRow {
  return {
    traceVersion: 2,
    requestId: 'req-1',
    at: '2026-01-01T00:00:00.000Z',
    method: 'POST',
    path: '/v1/messages',
    httpStatus: 200,
    ok: true,
    localAuth: { status: 'ok' },
    pool: { status: 'ok' },
    conversion: { status: 'ok', path: 'messages_to_anthropic' },
    upstreamAuth: { status: 'ok' },
    upstream: { status: 'ok' },
    profileId: 'route-1',
    sourceLabel: 'Route 1',
    ...partial,
  };
}

describe('ActivityMonitoringPanel', () => {
  it('shows an empty request table instead of the no-route empty prompt', () => {
    const markup = render(createElement(ActivityMonitoringPanel, { snapshot: snapshot() }));
    expect(markup).toContain('data-table-shell="default"');
    expect(markup).toContain('data-col="time"');
    expect(markup).toContain('还没有请求记录');
    expect(markup).not.toContain('连接池已有登录，等待创建或启动本机路由');
    expect(markup).not.toContain('连接池已有登录，但还没有本机路由');
    expect(markup).not.toContain('请打开路由概览');
    expect(markup).not.toContain('打开路由概览');
  });

  it('keeps the connection-pool empty state when there are no logins', () => {
    const markup = render(
      createElement(ActivityMonitoringPanel, {
        snapshot: snapshot({ kind: 'noLogins', hasEnrolledLogins: false }),
      }),
    );
    expect(markup).toContain('还没有可监控的登录');
    expect(markup).not.toContain('data-table-shell');
  });

  it('uses the selected request for the top five stages and falls back only without a selection', () => {
    const newest = row({ requestId: 'newest' });
    const selectedFailure = row({
      requestId: 'selected-failure',
      ok: false,
      httpStatus: 401,
      upstreamAuth: { status: 'failed', httpStatus: 401 },
      upstream: { status: 'pending' },
      failureStage: 'upstream_response',
    });
    const ready = snapshot({
      kind: 'ready',
      feed: [newest, selectedFailure],
      monitoredProfileIds: ['route-1'],
      runningCount: 1,
    });
    expect(render(createElement(ActivityMonitoringPanel, { snapshot: ready, activeId: 'selected-failure' })))
      .toContain('data-route-trace-legend-row="selected-failure"');
    expect(render(createElement(ActivityMonitoringPanel, { snapshot: ready })))
      .toContain('data-route-trace-legend-row="newest"');
  });
});
