import { describe, expect, it } from 'vitest';
import {
  buildTraceFlowView,
  conversionPathId,
  inferLocalEndpointKind,
  parseConversionPath,
  uniquePoolDisplayLabels,
  uniqueTraceUpstreamUrls,
} from './route-trace-visual-model';
import type { AdapterBridgeRouteTrace } from '@/lib/backend/contracts/adapter';

const baseTrace: AdapterBridgeRouteTrace = {
  requestId: 'req-1',
  at: '2026-01-01T00:00:00.000Z',
  method: 'POST',
  path: '/v1/messages',
  httpStatus: 200,
  ok: true,
  localEndpoint: { status: 'ok' },
  localAuth: { status: 'ok', profileId: 'p1', port: 43121 },
  pool: {
    status: 'ok',
    selectedMember: { label: 'acct-1', sourceKind: 'account', sourceId: 'a1' },
  },
  conversion: { status: 'ok', path: 'messages_to_anthropic', result: 'converted' },
  upstreamAuth: { status: 'ok', httpStatus: 200 },
  upstream: {
    status: 'ok',
    url: 'https://api.anthropic.com/v1/messages',
    member: { label: 'acct-1', sourceKind: 'account', sourceId: 'a1' },
    upstreamModel: 'claude-sonnet',
    httpStatus: 200,
  },
};

describe('route-trace-visual-model', () => {
  it('parses conversion path ids', () => {
    expect(parseConversionPath('messages_to_anthropic')).toEqual({
      row: 'messages',
      col: 'anthropic',
      passthrough: false,
    });
    expect(parseConversionPath('passthrough')).toEqual({
      row: null,
      col: null,
      passthrough: true,
    });
    expect(conversionPathId('responses', 'grok')).toBe('responses_to_grok');
  });

  it('infers local endpoint from request path', () => {
    expect(inferLocalEndpointKind({ ...baseTrace, path: '/v1/messages' })).toBe('messages');
    expect(inferLocalEndpointKind({
      ...baseTrace,
      path: '/v1/responses',
      conversion: { status: 'ok', path: 'responses_to_grok' },
    })).toBe('responses_grok');
  });

  it('marks the selected completed endpoint successful and lights the matrix cell', () => {
    const view = buildTraceFlowView(baseTrace);
    expect(view.activeEndpoint).toBe('messages');
    expect(view.endpoints.find((node) => node.kind === 'messages')?.state).toBe('ok');
    expect(view.conversion.activeRow).toBe('messages');
    expect(view.conversion.activeCol).toBe('anthropic');
    expect(view.conversion.result).toBe('converted');
    const activeCell = view.conversion.matrix.find(
      (cell) => cell.row === 'messages' && cell.col === 'anthropic',
    );
    expect(activeCell?.state).toBe('ok');
  });

  it('fails the inbound endpoint when the path is not served', () => {
    const view = buildTraceFlowView({
      ...baseTrace,
      path: '/v1/responses',
      httpStatus: 404,
      ok: false,
      localAuth: { status: 'ok', profileId: 'p1', port: 43121 },
      pool: { status: 'skipped' },
      conversion: {
        status: 'skipped',
        path: '',
        code: 'surface_mismatch',
        message: 'This route only serves /v1/messages',
      },
      upstreamAuth: { status: 'skipped' },
      upstream: { status: 'skipped' },
      failureStage: 'local_endpoint',
    });
    expect(view.localAuth.state).toBe('ok');
    expect(view.failureStage).toBe('local_endpoint');
    expect(view.endpoints.find((node) => node.kind === 'responses_codex')?.state).toBe('failed');
    expect(view.endpoints.find((node) => node.kind === 'messages')?.state).toBe('idle');
  });

  it('marks legacy traces as skipped stages', () => {
    const view = buildTraceFlowView({ ...baseTrace, legacySummary: true });
    expect(view.legacySummary).toBe(true);
    expect(view.localAuth.state).toBe('skipped');
    expect(view.endpoints.every((node) => node.state === 'skipped')).toBe(true);
  });

  it('exposes four local endpoints and unique pool / upstream preview values', () => {
    const view = buildTraceFlowView(baseTrace);
    expect(view.endpoints.map((node) => node.kind)).toEqual([
      'messages',
      'responses_codex',
      'responses_grok',
      'chat_completions',
    ]);
    expect(uniquePoolDisplayLabels([
      {
        members: [
          { displayLabel: 'Acct A' },
          { displayLabel: 'Acct A' },
          { displayLabel: '  ' },
          { displayLabel: 'Acct B' },
        ],
      },
    ])).toEqual(['Acct A', 'Acct B']);
    expect(uniqueTraceUpstreamUrls([
      { upstream: { url: 'https://api.anthropic.com/v1/messages' } },
      { upstream: { url: 'https://api.anthropic.com/v1/messages' } },
      { upstream: { url: ' https://api.x.ai/v1/responses ' } },
    ])).toEqual([
      'https://api.anthropic.com/v1/messages',
      'https://api.x.ai/v1/responses',
    ]);
  });
});
