import { createElement, type ReactElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { MemoryRouter } from 'react-router-dom';
import { describe, expect, it } from 'vitest';
import { TooltipProvider } from '@/components/ui/tooltip';
import { ActivityMonitoringPanel } from './ActivityMonitoringPanel';
import type { ActivityPageSnapshot } from './activity-view-model';

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

describe('ActivityMonitoringPanel', () => {
  it('shows an empty request table instead of the no-route empty prompt', () => {
    const markup = render(createElement(ActivityMonitoringPanel, { snapshot: snapshot() }));
    expect(markup).toContain('data-table-shell="default"');
    expect(markup).toContain('data-col="time"');
    expect(markup).toContain('还没有请求记录');
    expect(markup).toContain('连接池已有登录，等待创建或启动本机路由');
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
});
