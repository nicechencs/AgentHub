import { createElement, type ReactElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { TooltipProvider } from '@/components/ui/tooltip';
import type { DefaultRoutePoolOverview } from '@/lib/backend/contracts/adapter';
import { PoolSchedulePolicies, poolSchedulePolicy } from './PoolSchedulePolicyControl';

function pool(schedulePolicy: DefaultRoutePoolOverview['schedulePolicy']): DefaultRoutePoolOverview {
  return {
    id: 'pool-codex',
    targetAgentId: 'codex',
    surface: 'responses',
    dialect: 'codex',
    unifiedGatewayEnrolled: true,
    schedulePolicy,
    members: [],
  };
}

function render(node: ReactElement): string {
  return renderToStaticMarkup(node);
}

describe('pool schedule policy', () => {
  it('defaults a missing policy to priority failover', () => {
    expect(poolSchedulePolicy(pool(undefined))).toBe('priority_failover');
    expect(poolSchedulePolicy(pool('round_robin'))).toBe('round_robin');
  });

  it('shows both schedule choices and marks the current one', () => {
    const markup = render(createElement(
      TooltipProvider,
      null,
      createElement(PoolSchedulePolicies, {
        pools: [pool('round_robin')],
        onChanged: () => {},
      }),
    ));
    expect(markup).toContain('优先级故障转移');
    expect(markup).toContain('轮询');
    expect(markup).toContain('aria-selected="true"');
    expect(markup).toContain('轮询只在同一优先级');
  });
});
