import { describe, expect, it } from 'vitest';
import {
  buildTraceFlowView,
  conversionPathId,
  inferLocalEndpointKind,
  parseConversionPath,
} from './route-trace-visual-model';
import type { AdapterBridgeRouteTrace } from '@/lib/backend/contracts/adapter';

const baseTrace: AdapterBridgeRouteTrace = {
  requestId: 'req-1',
  at: '2026-01-01T00:00:00.000Z',
  method: 'POST',
  path: '/v1/messages',
  httpStatus: 200,
  ok: true,
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

  it('lights the active endpoint and matrix cell', () => {
    const view = buildTraceFlowView(baseTrace);
    expect(view.activeEndpoint).toBe('messages');
    expect(view.endpoints.find((node) => node.kind === 'messages')?.state).toBe('active');
    expect(view.conversion.activeRow).toBe('messages');
    expect(view.conversion.activeCol).toBe('anthropic');
    const activeCell = view.conversion.matrix.find(
      (cell) => cell.row === 'messages' && cell.col === 'anthropic',
    );
    expect(activeCell?.state).toBe('ok');
  });

  it('marks legacy traces as skipped stages', () => {
    const view = buildTraceFlowView({ ...baseTrace, legacySummary: true });
    expect(view.legacySummary).toBe(true);
    expect(view.localAuth.state).toBe('skipped');
    expect(view.endpoints.every((node) => node.state === 'skipped')).toBe(true);
  });
});
