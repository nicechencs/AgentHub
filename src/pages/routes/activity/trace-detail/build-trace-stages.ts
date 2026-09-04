import type { TranslateFn } from '@/lib/i18n';
import type { RouteTraceListItem } from '@/components/shared/RouteTraceList';
import type { RouteTraceStageId } from '@/lib/backend/contracts/adapter';
import { routeEndpointHttpParts } from '@/lib/route-endpoints';
import { formatInboundAt } from '@/pages/routes/shared/route-endpoint-copy';
import {
  activityTraceConversionLabel,
  activityTraceInboundEndpoint,
  activityTraceKeyParts,
  activityTraceLocalBrand,
  activityTraceModelLabel,
  activityTraceStageStatusLabel,
  activityTraceUpstreamEndpoint,
  formatTraceSeconds,
  formatTraceTokens,
  type ActivityTraceKeyToken,
} from '../activity-trace-list-model';
import { TRACE_STAGE_REGISTRY } from './trace-stage-registry';
import type { DetailStageStatus, TraceStageDetail, TraceStageViewModel } from './trace-stage-types';

function keyHint(last4?: string | null): string {
  return last4?.trim() ? `••••${last4.trim()}` : '—';
}

function localKeyHint(
  row: RouteTraceListItem,
  keyTokens: readonly ActivityTraceKeyToken[],
): string {
  return activityTraceKeyParts(row, keyTokens).label || keyHint(row.localAuth.keyLast4);
}

function responseResultLabel(value: string | null | undefined, t: TranslateFn): string {
  if (value === 'completed') return t('routes.trace.detail.completed');
  if (value === 'streaming') return t('routes.trace.detail.streaming');
  if (value === 'failed') return t('routes.inbound.fail');
  if (value === 'interrupted') return t('routes.trace.detail.interrupted');
  return value || '—';
}

function completionLabel(value: string | null | undefined, t: TranslateFn): string {
  if (value === 'response_returned') return t('routes.trace.detail.responseReturned');
  if (value === 'stream_completed') return t('routes.trace.detail.streamCompleted');
  if (value === 'stream_error') return t('routes.trace.detail.streamError');
  if (value === 'client_disconnected') return t('routes.trace.detail.clientDisconnected');
  if (value === 'streaming') return t('routes.trace.detail.streaming');
  return value || t('routes.trace.detail.noSeparateData');
}

function text(label: string, value: unknown, mono = false): TraceStageDetail {
  return { kind: 'text', label, value: value == null || value === '' ? '—' : String(value), mono };
}

function addedStageStatus(
  traceVersion: number,
  status: DetailStageStatus | null | undefined,
): DetailStageStatus {
  if (traceVersion < 2 || status == null) return 'unrecorded';
  return status;
}

function attemptDurationLabel(value: number | null | undefined, t: TranslateFn): string | null {
  if (value == null) return null;
  return value < 1000 ? `${value} ms` : formatTraceSeconds(value, t);
}

function authResultLabel(value: string | null | undefined, t: TranslateFn): string {
  if (value === 'accepted') return t('routes.inbound.ok');
  if (value === 'rejected') return t('routes.inbound.fail');
  return t('routes.trace.detail.unrecorded');
}

export function buildTraceStages(
  row: RouteTraceListItem,
  t: TranslateFn,
  keyTokens: readonly ActivityTraceKeyToken[] = [],
): TraceStageViewModel[] {
  const inbound = routeEndpointHttpParts({ path: row.path, port: row.localAuth.port });
  const selectedLogin = row.upstream.member ?? row.pool.selectedMember;
  const model = activityTraceModelLabel(row);
  const localBrand = activityTraceLocalBrand(row);
  const duration = formatTraceSeconds(row.latencyMs, t);
  const firstToken = formatTraceSeconds(row.ttftMs, t);
  const tokens = formatTraceTokens(row.inputTokens, row.outputTokens, t);
  const attempts = row.pool.attempts ?? [];
  const detailsById: Record<RouteTraceStageId, TraceStageDetail[]> = {
    received: [
      text(t('routes.trace.detail.requestId'), row.requestId, true),
      text(t('routes.activity.colTime'), formatInboundAt(row.at), true),
      text(t('routes.activity.colRequest'), `${row.method} ${row.path}`),
    ],
    local_auth: [
      text(t('routes.activity.localKey'), localKeyHint(row, keyTokens), true),
      text(t('routes.trace.detail.port'), row.localAuth.port),
      text(t('routes.trace.detail.code'), row.localAuth.code),
      text(t('routes.trace.detail.message'), row.localAuth.message),
    ],
    local_endpoint: [
      {
        kind: 'endpoint' as const,
        label: t('routes.activity.inboundEndpoint'),
        path: inbound.path,
        port: row.localAuth.port,
        host: inbound.host,
        endpointId: inbound.endpointId,
        brandAgentId: localBrand,
      },
      text(t('routes.trace.detail.code'), row.localEndpoint?.code),
      text(t('routes.trace.detail.message'), row.localEndpoint?.message),
    ],
    admission: [
      text(t('routes.trace.detail.code'), row.admission?.code),
      text(
        t('routes.trace.detail.message'),
        row.admission?.message || t('routes.trace.detail.noSeparateData'),
      ),
    ],
    route_resolution: [
      text(t('routes.activity.colModel'), model),
      text(t('routes.trace.detail.code'), row.routeResolution?.code),
      text(t('routes.trace.detail.message'), row.routeResolution?.message),
    ],
    pool: [
      text(t('routes.activity.selectedLogin'), selectedLogin?.label),
      text(t('routes.activity.poolModel'), row.upstream.upstreamModel || model),
      text(t('routes.activity.upstreamKey'), keyHint(selectedLogin?.keyLast4), true),
      ...attempts.map((attempt, index): TraceStageDetail => ({
        kind: 'attempt',
        label: t('routes.trace.attempt', { n: attempt.attemptId ?? index + 1 }),
        member: attempt.member.label,
        status: activityTraceStageStatusLabel(attempt.status, t),
        requestStatus: attempt.requestStatus
          ? `${t('routes.trace.detailStage.upstreamRequest')}: ${activityTraceStageStatusLabel(attempt.requestStatus, t)}`
          : null,
        responseStatus: attempt.responseStatus
          ? `${t('routes.trace.detailStage.upstreamResponse')}: ${activityTraceStageStatusLabel(attempt.responseStatus, t)}`
          : null,
        url: attempt.url,
        httpStatus: attempt.httpStatus,
        authResult: authResultLabel(attempt.authResult, t),
        duration: attemptDurationLabel(attempt.durationMs, t),
        code: attempt.code,
        message: attempt.message,
      })),
      text(t('routes.trace.detail.code'), row.pool.code),
      text(t('routes.trace.detail.message'), row.pool.message),
    ],
    request_conversion: [
      text(t('routes.trace.path'), row.conversion.path),
      text(t('routes.trace.result'), row.conversion.result),
      text(t('routes.trace.detail.code'), row.conversion.code),
      text(t('routes.trace.detail.message'), row.conversion.message),
    ],
    upstream_request: [
      text(t('routes.activity.outboundEndpoint'), row.upstreamRequest?.url, true),
      text(t('routes.activity.upstreamAuthLogin'), row.upstreamRequest?.member?.label),
      text(t('routes.activity.upstreamKey'), keyHint(row.upstreamRequest?.member?.keyLast4), true),
      text(t('routes.activity.upstreamModel'), row.upstreamRequest?.model),
      text(t('routes.trace.detail.code'), row.upstreamRequest?.code),
      text(t('routes.trace.detail.message'), row.upstreamRequest?.message),
    ],
    upstream_response: [
      text(t('routes.trace.detail.httpStatus'), row.upstream.httpStatus),
      text(
        t('routes.trace.detail.authResult'),
        `${activityTraceStageStatusLabel(row.upstreamAuth.status, t)}${row.upstreamAuth.httpStatus != null ? ` · ${row.upstreamAuth.httpStatus}` : ''}`,
      ),
      text(t('routes.trace.detail.code'), row.upstream.code || row.upstreamAuth.code),
      text(t('routes.trace.detail.message'), row.upstream.message || row.upstreamAuth.message),
    ],
    response_conversion: [
      text(t('routes.trace.path'), row.responseConversion?.path),
      text(t('routes.trace.result'), responseResultLabel(row.responseConversion?.result, t)),
      text(t('routes.trace.detail.code'), row.responseConversion?.code),
      text(t('routes.trace.detail.message'), row.responseConversion?.message),
    ],
    delivery: [
      text(t('routes.trace.detail.httpStatus'), row.delivery?.httpStatus ?? row.httpStatus),
      text(t('routes.trace.detail.stream'), row.delivery?.stream ? t('routes.trace.detail.streaming') : t('routes.trace.detail.notStreaming')),
      text(t('routes.trace.detail.completion'), completionLabel(row.delivery?.completion, t)),
      text(t('routes.activity.colDuration'), duration),
      text(t('routes.activity.colFirstToken'), firstToken),
      text(t('routes.activity.colTokens'), tokens),
    ],
  };

  const statusById: Record<RouteTraceStageId, DetailStageStatus> = {
    received: 'ok',
    local_auth: row.localAuth.status,
    local_endpoint: addedStageStatus(row.traceVersion, row.localEndpoint?.status),
    admission: addedStageStatus(row.traceVersion, row.admission?.status),
    route_resolution: addedStageStatus(row.traceVersion, row.routeResolution?.status),
    pool: row.pool.status,
    request_conversion: row.conversion.status,
    upstream_request: addedStageStatus(row.traceVersion, row.upstreamRequest?.status),
    upstream_response: row.upstreamAuth.status === 'failed' ? 'failed' : row.upstream.status,
    response_conversion: addedStageStatus(row.traceVersion, row.responseConversion?.status),
    delivery: addedStageStatus(row.traceVersion, row.delivery?.status),
  };
  const summaryById: Record<RouteTraceStageId, string | null | undefined> = {
    received: `${row.method} ${row.path}`,
    local_auth: localKeyHint(row, keyTokens),
    local_endpoint: activityTraceInboundEndpoint(row) || row.path,
    admission: row.admission?.message,
    route_resolution: model || null,
    pool: selectedLogin?.label,
    request_conversion: activityTraceConversionLabel(row, t),
    upstream_request: activityTraceUpstreamEndpoint(row) || row.upstreamRequest?.url,
    upstream_response: row.upstream.httpStatus != null ? String(row.upstream.httpStatus) : null,
    response_conversion: responseResultLabel(row.responseConversion?.result, t),
    delivery: row.delivery ? completionLabel(row.delivery.completion, t) : null,
  };

  return TRACE_STAGE_REGISTRY.map(({ id, titleKey }) => ({
    id,
    title: t(titleKey),
    status: statusById[id],
    summary: summaryById[id],
    details: detailsById[id],
  }));
}
