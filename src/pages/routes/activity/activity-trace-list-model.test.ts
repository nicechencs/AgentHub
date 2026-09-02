import { describe, expect, it } from 'vitest';
import { translate } from '@/lib/i18n';
import {
  ACTIVITY_TRACE_STAGES,
  ACTIVITY_TRACE_WIDTH_SPECS,
  activityTraceColumnLabel,
  activityTraceModelLabel,
  activityTraceStageLabel,
  activityTraceStageStatusLabel,
  formatTraceSeconds,
  formatTraceTokens,
} from './activity-trace-list-model';

const t = (key: Parameters<typeof translate>[1], params?: Parameters<typeof translate>[2]) =>
  translate('zh', key, params);

describe('activity-trace-list-model', () => {
  it('covers the monitoring columns and five stages', () => {
    expect(ACTIVITY_TRACE_WIDTH_SPECS.map((spec) => spec.key)).toEqual([
      'time',
      'request',
      'model',
      'firstToken',
      'duration',
      'tokens',
      'stages',
      'route',
      'details',
    ]);
    expect([...ACTIVITY_TRACE_STAGES]).toEqual([
      'local_auth',
      'pool',
      'conversion',
      'upstream_auth',
      'upstream',
    ]);
    expect(activityTraceColumnLabel('time', t)).toBe('时间');
    expect(activityTraceColumnLabel('request', t)).toBe('请求');
    expect(activityTraceColumnLabel('model', t)).toBe('模型');
    expect(activityTraceColumnLabel('firstToken', t)).toBe('首字');
    expect(activityTraceColumnLabel('duration', t)).toBe('请求时长');
    expect(activityTraceColumnLabel('tokens', t)).toBe('Token');
    expect(activityTraceColumnLabel('stages', t)).toBe('五段');
    expect(activityTraceColumnLabel('route', t)).toBe('路由');
    expect(activityTraceColumnLabel('details', t)).toBe('详情');
    expect(activityTraceStageLabel('local_auth', t)).toBe('本机鉴权');
    expect(activityTraceStageStatusLabel('ok', t)).toBe('成功');
    expect(activityTraceStageStatusLabel('failed', t)).toBe('失败');
    expect(activityTraceStageStatusLabel('skipped', t)).toBe('未到达');
  });

  it('formats first-token and request duration in seconds', () => {
    expect(formatTraceSeconds(800, t)).toBe('0.8s');
    expect(formatTraceSeconds(4200, t)).toBe('4.2s');
    expect(formatTraceSeconds(12_000, t)).toBe('12s');
    expect(formatTraceSeconds(null, t)).toBe('');
  });

  it('formats consumed tokens and prefers the request model', () => {
    expect(formatTraceTokens(1200, 340, t)).toBe('1.2K / 340');
    expect(formatTraceTokens(null, null, t)).toBe('');
    expect(activityTraceModelLabel({ model: 'claude-sonnet' })).toBe('claude-sonnet');
    expect(activityTraceModelLabel({
      upstream: { upstreamModel: 'gpt-5' },
    })).toBe('gpt-5');
  });
});
