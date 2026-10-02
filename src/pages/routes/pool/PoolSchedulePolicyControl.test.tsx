import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { DefaultRoutePoolOverview, RouteSchedulePolicy } from '@/lib/backend/contracts/adapter';

const testState = vi.hoisted(() => ({
  controls: [] as Array<{
    value: string;
    onChange: (value: string) => void;
    options: Array<{ value: string; disabled?: boolean }>;
  }>,
  setRoutePoolSchedulePolicy: vi.fn(),
  toast: vi.fn(),
}));

vi.mock('@/lib/api/adapter', () => ({
  setRoutePoolSchedulePolicy: testState.setRoutePoolSchedulePolicy,
}));

vi.mock('@/components/shared/LanguageProvider', () => ({
  useI18n: () => ({
    t: (key: string, params?: { count?: number }) => {
      const labels: Record<string, string> = {
        'routes.pool.schedule.label': '调度',
        'routes.pool.schedule.hint': '调度规则',
        'routes.pool.schedule.mixedHint': '当前连接池的调度不同，请选择后统一设置。',
        'routes.pool.schedule.priorityFailover': '按顺序用，出错换下一个',
        'routes.pool.schedule.priorityFailoverHint': '优先级',
        'routes.pool.schedule.roundRobin': '轮流用',
        'routes.pool.schedule.roundRobinHint': '轮流',
        'routes.pool.schedule.saved': '调度已更新',
        'routes.pool.schedule.partialSaveFailed': `失败 ${params?.count ?? 0} 个连接池`,
      };
      return labels[key] ?? key;
    },
  }),
}));

vi.mock('@/components/ui/toast', () => ({
  useToast: () => ({ toast: testState.toast }),
}));

vi.mock('@/components/shared/SegmentedControl', () => ({
  SegmentedControl: (props: (typeof testState.controls)[number]) => {
    testState.controls.push(props);
    return null;
  },
}));

import {
  applyPoolSchedulePolicy,
  combinedPoolSchedulePolicy,
  poolSchedulePolicy,
  PoolSchedulePolicies,
} from './PoolSchedulePolicyControl';

function pool(
  schedulePolicy: DefaultRoutePoolOverview['schedulePolicy'],
  id = 'pool-codex',
): DefaultRoutePoolOverview {
  return {
    id,
    targetAgentId: 'codex',
    surface: 'responses',
    dialect: 'codex',
    unifiedGatewayEnrolled: true,
    schedulePolicy,
    members: [],
  };
}

function render(
  pools: readonly DefaultRoutePoolOverview[],
  onChanged: () => void = vi.fn(),
): string {
  return renderToStaticMarkup(createElement(PoolSchedulePolicies, {
    pools,
    onChanged,
  }));
}

describe('pool schedule policy', () => {
  beforeEach(() => {
    testState.controls.length = 0;
    testState.setRoutePoolSchedulePolicy.mockReset();
    testState.setRoutePoolSchedulePolicy.mockResolvedValue(undefined);
    testState.toast.mockReset();
  });

  it('defaults a missing policy to priority failover', () => {
    expect(poolSchedulePolicy(pool(undefined))).toBe('priority_failover');
    expect(poolSchedulePolicy(pool('round_robin'))).toBe('round_robin');
  });

  it('shows one shared control when all pools have the same policy', () => {
    render([pool('round_robin', 'pool-a'), pool('round_robin', 'pool-b')]);

    expect(testState.controls).toHaveLength(1);
    expect(testState.controls[0]?.value).toBe('round_robin');
    expect(testState.controls[0]?.options).toHaveLength(2);
    expect(combinedPoolSchedulePolicy([
      pool('round_robin', 'pool-a'),
      pool('round_robin', 'pool-b'),
    ])).toBe('round_robin');
  });

  it('shows no selected option and a hint when pool policies differ', () => {
    const markup = render([pool('round_robin', 'pool-a'), pool('priority_failover', 'pool-b')]);

    expect(testState.controls).toHaveLength(1);
    expect(testState.controls[0]?.value).toBe('');
    expect(markup).toContain('当前连接池的调度不同，请选择后统一设置。');
    expect(combinedPoolSchedulePolicy([
      pool('round_robin', 'pool-a'),
      pool('priority_failover', 'pool-b'),
    ])).toBeNull();
  });

  it('updates each distinct pool and reports an all-success result', async () => {
    const update = vi.fn().mockResolvedValue(undefined);
    const result = await applyPoolSchedulePolicy([
      pool('priority_failover', 'pool-a'),
      pool('priority_failover', 'pool-a'),
      pool('priority_failover', 'pool-b'),
    ], 'round_robin', update);

    expect(update).toHaveBeenCalledTimes(2);
    expect(update.mock.calls).toEqual([
      ['pool-a', 'round_robin'],
      ['pool-b', 'round_robin'],
    ]);
    expect(result).toEqual({ attempted: 2, succeeded: 2, failed: 0 });
  });

  it('counts partial failures without exposing pool ids', async () => {
    const update = vi.fn((id: string, _policy: RouteSchedulePolicy) => (
      id === 'pool-b' ? Promise.reject(new Error('unavailable')) : Promise.resolve()
    ));
    const result = await applyPoolSchedulePolicy([
      pool('priority_failover', 'pool-a'),
      pool('priority_failover', 'pool-b'),
    ], 'round_robin', update);

    expect(result).toEqual({ attempted: 2, succeeded: 1, failed: 1 });
  });

  it('refreshes after a partial batch and reports only the failure count', async () => {
    const onChanged = vi.fn();
    testState.setRoutePoolSchedulePolicy.mockImplementation((id: string) => (
      id === 'pool-b' ? Promise.reject(new Error('unavailable')) : Promise.resolve()
    ));
    render([
      pool('priority_failover', 'pool-a'),
      pool('priority_failover', 'pool-b'),
    ], onChanged);
    testState.controls[0]?.onChange('round_robin');
    await new Promise<void>((resolve) => setTimeout(resolve, 0));

    expect(onChanged).toHaveBeenCalledTimes(1);
    expect(testState.toast).toHaveBeenCalledWith({
      title: '失败 1 个连接池',
      variant: 'danger',
    });
    expect(JSON.stringify(testState.toast.mock.calls)).not.toContain('pool-b');
  });

  it('does not start a second batch while the first batch is pending', async () => {
    let release!: () => void;
    const pending = new Promise<void>((resolve) => { release = resolve; });
    testState.setRoutePoolSchedulePolicy.mockReturnValue(pending);
    render([pool('priority_failover', 'pool-a'), pool('priority_failover', 'pool-b')]);
    const onChange = testState.controls[0]?.onChange;

    onChange?.('round_robin');
    onChange?.('priority_failover');

    await Promise.resolve();
    expect(testState.setRoutePoolSchedulePolicy).toHaveBeenCalledTimes(2);
    release();
    await pending;
  });

  it('renders nothing when there are no pools', () => {
    expect(render([])).toBe('');
    expect(testState.controls).toHaveLength(0);
  });
});
