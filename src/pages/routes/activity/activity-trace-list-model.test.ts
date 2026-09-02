import { describe, expect, it } from 'vitest';
import { translate } from '@/lib/i18n';
import {
  ACTIVITY_TRACE_STAGES,
  ACTIVITY_TRACE_WIDTH_SPECS,
  activityTraceColumnLabel,
  activityTraceStageLabel,
  activityTraceStageStatusLabel,
} from './activity-trace-list-model';

const t = (key: Parameters<typeof translate>[1], params?: Parameters<typeof translate>[2]) =>
  translate('zh', key, params);

describe('activity-trace-list-model', () => {
  it('covers the six monitoring columns and five stages', () => {
    expect(ACTIVITY_TRACE_WIDTH_SPECS.map((spec) => spec.key)).toEqual([
      'time',
      'request',
      'result',
      'stages',
      'route',
      'latency',
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
    expect(activityTraceColumnLabel('result', t)).toBe('结果');
    expect(activityTraceColumnLabel('stages', t)).toBe('五段');
    expect(activityTraceColumnLabel('route', t)).toBe('路由');
    expect(activityTraceColumnLabel('latency', t)).toBe('延迟');
    expect(activityTraceStageLabel('local_auth', t)).toBe('本机鉴权');
    expect(activityTraceStageStatusLabel('ok', t)).toBe('成功');
    expect(activityTraceStageStatusLabel('failed', t)).toBe('失败');
    expect(activityTraceStageStatusLabel('skipped', t)).toBe('未执行');
  });
});
