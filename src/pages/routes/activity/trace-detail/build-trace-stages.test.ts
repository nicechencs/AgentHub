import { describe, expect, it } from 'vitest';
import type { RouteTraceListItem } from '@/components/shared/RouteTraceList';
import type { TranslateFn } from '@/lib/i18n';
import { buildTraceStages } from './build-trace-stages';
import { TRACE_STAGE_REGISTRY } from './trace-stage-registry';

const t = ((key: string) => key) as TranslateFn;

function trace(overrides: Partial<RouteTraceListItem> = {}): RouteTraceListItem {
  return {
    traceVersion: 2,
    requestId: 'req-1',
    at: '2026-03-01T00:00:00.000Z',
    method: 'POST',
    path: '/v1/messages',
    httpStatus: 200,
    ok: true,
    localEndpoint: { status: 'ok' },
    localAuth: { status: 'ok', keyLast4: '1234', port: 8787 },
    admission: { status: 'ok' },
    routeResolution: { status: 'ok' },
    pool: { status: 'ok', attempts: [] },
    conversion: { status: 'ok', path: 'passthrough' },
    upstreamAuth: { status: 'ok', httpStatus: 200 },
    upstreamRequest: { status: 'ok', url: 'https://example.com/v1/messages' },
    upstream: { status: 'ok', httpStatus: 200 },
    responseConversion: { status: 'ok', path: 'passthrough', result: 'completed' },
    delivery: { status: 'ok', stream: false, completion: 'response_returned' },
    ...overrides,
  };
}

describe('buildTraceStages', () => {
  it('uses the one registry order for all eleven nodes', () => {
    const stages = buildTraceStages(trace(), t);
    expect(stages).toHaveLength(11);
    expect(stages.map((stage) => stage.id)).toEqual(
      TRACE_STAGE_REGISTRY.map((stage) => stage.id),
    );
  });

  it('does not infer the upstream request node from the response node', () => {
    const stages = buildTraceStages(trace({ upstreamRequest: undefined }), t);
    expect(stages.find((stage) => stage.id === 'upstream_request')?.status).toBe('unrecorded');
  });

  it('marks fields added after v1 as unrecorded for restored legacy traces', () => {
    const stages = buildTraceStages(trace({
      traceVersion: 1,
      upstreamRequest: { status: 'pending' },
      responseConversion: { status: 'pending', path: '' },
    }), t);
    expect(stages.find((stage) => stage.id === 'upstream_request')?.status).toBe('unrecorded');
    expect(stages.find((stage) => stage.id === 'response_conversion')?.status).toBe('unrecorded');
  });

  it('preserves interrupted as a first-class response conversion status', () => {
    const stages = buildTraceStages(trace({
      ok: false,
      responseConversion: { status: 'interrupted', path: 'anthropic_to_messages', result: 'interrupted' },
      delivery: { status: 'failed', stream: true, completion: 'client_disconnected' },
      failureStage: 'delivery',
    }), t);
    expect(stages.find((stage) => stage.id === 'response_conversion')?.status).toBe('interrupted');
    expect(stages.find((stage) => stage.id === 'delivery')?.status).toBe('failed');
  });
});
