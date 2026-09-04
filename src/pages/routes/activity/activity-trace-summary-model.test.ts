import { describe, expect, it } from 'vitest';
import { translate } from '@/lib/i18n';
import type { RouteTraceListItem } from '@/components/shared/RouteTraceList';
import {
  activityTraceFailureHeadline,
  activityTraceResultLabel,
  activityTraceDisplayRow,
  activityTraceStageStatus,
  selectedActivityTrace,
  summarizeActivityTrace,
} from './activity-trace-summary-model';

const t = (key: Parameters<typeof translate>[1], params?: Parameters<typeof translate>[2]) =>
  translate('zh', key, params);

function row(partial: Partial<RouteTraceListItem> = {}): RouteTraceListItem {
  return {
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
    failureStage: null,
    ...partial,
  };
}

describe('activity-trace-summary-model', () => {
  it('marks a completed successful request as successful', () => {
    const summary = summarizeActivityTrace(row());
    expect(summary).toMatchObject({ result: 'success', failureStage: null });
    expect(activityTraceResultLabel(summary, t)).toBe('成功');
  });

  it('does not show a completed successful request as waiting', () => {
    const success = row({ upstream: { status: 'pending' } });
    expect(activityTraceStageStatus(success, 'upstream')).toBe('skipped');
    expect(activityTraceDisplayRow(success)?.upstream.status).toBe('skipped');
  });

  it('reports an upstream-auth failure and marks later stages as not reached', () => {
    const failed = row({
      ok: false,
      httpStatus: 401,
      upstreamAuth: { status: 'failed', httpStatus: 401 },
      upstream: { status: 'pending' },
      failureStage: 'upstream_auth',
    });
    const summary = summarizeActivityTrace(failed);
    expect(summary).toMatchObject({ result: 'failed', failureStage: 'upstream_auth', errorMessage: '401' });
    expect(activityTraceResultLabel(summary, t)).toBe('失败于 上游鉴权');
    expect(activityTraceFailureHeadline(summary, t)).toBe('上游鉴权失败');
    expect(activityTraceStageStatus(failed, 'upstream')).toBe('skipped');
    expect(activityTraceDisplayRow(failed)?.upstream.status).toBe('skipped');
  });

  it('uses an HTTP status instead of exposing a raw error code when no message exists', () => {
    const failed = row({
      ok: false,
      httpStatus: 401,
      upstreamAuth: { status: 'failed', httpStatus: 401, code: 'unauthorized' },
      upstream: { status: 'pending' },
      failureStage: 'upstream_auth',
    });
    expect(summarizeActivityTrace(failed).errorMessage).toBe('401');
  });

  it('reports an upstream failure without guessing a check object', () => {
    const failed = row({
      ok: false,
      httpStatus: 502,
      upstream: { status: 'failed', httpStatus: 502, message: 'Bad gateway' },
      failureStage: 'upstream',
    });
    const summary = summarizeActivityTrace(failed);
    expect(summary).toMatchObject({ result: 'failed', failureStage: 'upstream', errorMessage: 'Bad gateway' });
    expect(activityTraceResultLabel(summary, t)).toBe('失败于 上游');
  });

  it('keeps a local endpoint failure explicit without changing the five-stage path', () => {
    const failed = row({
      ok: false,
      httpStatus: 400,
      localAuth: { status: 'ok' },
      pool: { status: 'skipped' },
      conversion: { status: 'skipped', path: '', code: 'invalid_path', message: 'Bad request' },
      upstreamAuth: { status: 'skipped' },
      upstream: { status: 'skipped' },
      failureStage: 'local_endpoint',
    });
    const summary = summarizeActivityTrace(failed);
    expect(summary).toMatchObject({ result: 'failed', failureStage: 'local_endpoint', errorMessage: 'Bad request' });
    expect(activityTraceResultLabel(summary, t)).toBe('失败于 本地调用端点');
    expect(activityTraceFailureHeadline(summary, t)).toBe('本地调用端点失败');
    expect(activityTraceStageStatus(failed, 'local_auth')).toBe('ok');
    expect(activityTraceStageStatus(failed, 'pool')).toBe('skipped');
    expect(activityTraceStageStatus(failed, 'conversion')).toBe('skipped');
  });

  it('reports failures from the expanded lifecycle', () => {
    const failed = row({
      ok: false,
      httpStatus: 400,
      routeResolution: { status: 'failed', code: 'model_unavailable', message: 'No route' },
      pool: { status: 'skipped' },
      conversion: { status: 'skipped', path: '' },
      upstreamAuth: { status: 'skipped' },
      upstream: { status: 'skipped' },
      failureStage: 'route_resolution',
    });
    const summary = summarizeActivityTrace(failed);
    expect(summary).toMatchObject({ result: 'failed', failureStage: 'route_resolution', errorMessage: 'No route' });
    expect(activityTraceResultLabel(summary, t)).toBe('失败于 模型与路由解析');
  });

  it('keeps legacy failures unknown and never shows a completed request as waiting', () => {
    const legacy = row({
      ok: false,
      httpStatus: 500,
      localAuth: { status: 'pending' },
      pool: { status: 'pending' },
      conversion: { status: 'pending', path: '' },
      upstreamAuth: { status: 'pending' },
      upstream: { status: 'pending' },
      failureStage: 'upstream',
      legacySummary: true,
    });
    const summary = summarizeActivityTrace(legacy);
    expect(summary.result).toBe('failureUnknown');
    expect(activityTraceResultLabel(summary, t)).toBe('无法确定失败环节');
    expect(activityTraceStageStatus(legacy, 'pool')).toBe('skipped');
  });

  it('returns the same selected row for the header and detail consumers', () => {
    const success = row({ requestId: 'success' });
    const failure = row({ requestId: 'failure', ok: false, failureStage: 'upstream' });
    expect(selectedActivityTrace([success, failure], 'failure')).toBe(failure);
    expect(selectedActivityTrace([success, failure], 'missing')).toBeNull();
  });
});
