import type {
  AdapterBridgeRouteTrace,
  RouteTraceStageStatus,
} from '@/lib/backend/contracts/adapter';
import type { TranslateFn } from '@/lib/i18n';
import {
  ACTIVITY_TRACE_STAGES,
  activityTraceStageLabel,
  type ActivityTraceStageId,
} from './activity-trace-list-model';

type ActivityTrace = Pick<
  AdapterBridgeRouteTrace,
  'ok' | 'httpStatus' | 'failureStage' | 'localEndpoint' | 'localAuth' | 'admission'
  | 'routeResolution' | 'pool' | 'conversion' | 'upstreamAuth' | 'upstream'
  | 'responseConversion' | 'delivery'
> & {
  legacySummary?: boolean;
};

export type ActivityTraceResult = 'success' | 'failed' | 'failureUnknown';
export type ActivityTraceFailureStageId =
  | 'local_endpoint'
  | 'admission'
  | 'route_resolution'
  | 'response_conversion'
  | 'delivery'
  | ActivityTraceStageId;

export type ActivityTraceSummary = {
  result: ActivityTraceResult;
  failureStage: ActivityTraceFailureStageId | null;
  errorMessage: string | null;
};

/** Finds the one row shared by the selected table item, header, and detail panel. */
export function selectedActivityTrace<Row extends { requestId: string }>(
  rows: readonly Row[],
  activeId: string | null | undefined,
): Row | null {
  if (!activeId) return null;
  return rows.find((row) => row.requestId === activeId) ?? null;
}

function isActivityTraceStage(stage: string | null | undefined): stage is ActivityTraceStageId {
  return ACTIVITY_TRACE_STAGES.some((candidate) => candidate === stage);
}

function isActivityTraceFailureStage(
  stage: string | null | undefined,
): stage is ActivityTraceFailureStageId {
  return stage === 'local_endpoint'
    || stage === 'admission'
    || stage === 'route_resolution'
    || stage === 'response_conversion'
    || stage === 'delivery'
    || isActivityTraceStage(stage);
}

function rawStageStatus(trace: ActivityTrace, stage: ActivityTraceStageId): RouteTraceStageStatus {
  if (stage === 'local_auth') return trace.localAuth.status;
  if (stage === 'pool') return trace.pool.status;
  if (stage === 'conversion') return trace.conversion.status;
  if (stage === 'upstream_auth') return trace.upstreamAuth.status;
  return trace.upstream.status;
}

function recordedFailureStage(trace: ActivityTrace): ActivityTraceFailureStageId | null {
  if (trace.ok || trace.legacySummary) return null;
  if (isActivityTraceFailureStage(trace.failureStage)) return trace.failureStage;
  return ACTIVITY_TRACE_STAGES.find((stage) => rawStageStatus(trace, stage) === 'failed') ?? null;
}

function stageError(trace: ActivityTrace, stage: ActivityTraceFailureStageId): string | null {
  const detail = stage === 'local_endpoint'
    ? trace.localEndpoint ?? trace.conversion
    : stage === 'admission'
      ? trace.admission ?? trace.localAuth
      : stage === 'route_resolution'
        ? trace.routeResolution ?? trace.pool
        : stage === 'local_auth'
          ? trace.localAuth
          : stage === 'pool'
            ? trace.pool
            : stage === 'conversion'
              ? trace.conversion
              : stage === 'upstream_auth'
                ? trace.upstreamAuth
                : stage === 'response_conversion'
                  ? trace.responseConversion ?? trace.upstream
                  : stage === 'delivery'
                    ? trace.delivery ?? trace.upstream
                    : trace.upstream;
  const message = detail.message?.trim();
  if (message) return message;
  if ('httpStatus' in detail && detail.httpStatus != null) return String(detail.httpStatus);
  if (trace.httpStatus >= 400) return String(trace.httpStatus);
  return detail.code?.trim() || null;
}

/** One source of truth for result, failure position, and stage display. */
export function summarizeActivityTrace(trace: ActivityTrace): ActivityTraceSummary {
  if (trace.ok) return { result: 'success', failureStage: null, errorMessage: null };
  const failureStage = recordedFailureStage(trace);
  if (!failureStage) {
    return {
      result: 'failureUnknown',
      failureStage: null,
      errorMessage: trace.httpStatus >= 400 ? String(trace.httpStatus) : null,
    };
  }
  return {
    result: 'failed',
    failureStage,
    errorMessage: stageError(trace, failureStage),
  };
}

/** Converts a completed trace into final five-stage display states. */
export function activityTraceStageStatus(
  trace: ActivityTrace,
  stage: ActivityTraceStageId,
): RouteTraceStageStatus {
  const summary = summarizeActivityTrace(trace);
  const raw = rawStageStatus(trace, stage);
  if (summary.result === 'success') return raw === 'pending' ? 'skipped' : raw;
  if (summary.failureStage && isActivityTraceStage(summary.failureStage)) {
    const stageIndex = ACTIVITY_TRACE_STAGES.indexOf(stage);
    const failureIndex = ACTIVITY_TRACE_STAGES.indexOf(summary.failureStage);
    if (stageIndex === failureIndex) return 'failed';
    if (stageIndex > failureIndex || raw === 'pending') return 'skipped';
    return raw;
  }
  return raw === 'pending' ? 'skipped' : raw;
}

/** A trace copy for the top five-stage view, using the same final states as list and detail. */
export function activityTraceDisplayRow<Row extends ActivityTrace>(trace: Row | undefined): Row | undefined {
  if (!trace) return trace;
  return {
    ...trace,
    localAuth: { ...trace.localAuth, status: activityTraceStageStatus(trace, 'local_auth') },
    pool: { ...trace.pool, status: activityTraceStageStatus(trace, 'pool') },
    conversion: { ...trace.conversion, status: activityTraceStageStatus(trace, 'conversion') },
    upstreamAuth: { ...trace.upstreamAuth, status: activityTraceStageStatus(trace, 'upstream_auth') },
    upstream: { ...trace.upstream, status: activityTraceStageStatus(trace, 'upstream') },
  } as Row;
}

export function activityTraceResultLabel(summary: ActivityTraceSummary, t: TranslateFn): string {
  if (summary.result === 'success') return t('routes.inbound.ok');
  if (summary.failureStage) {
    return t('routes.trace.failedAt', { stage: activityTraceFailureStageLabel(summary.failureStage, t) });
  }
  return t('routes.trace.failureUnknown');
}

export function activityTraceFailureHeadline(summary: ActivityTraceSummary, t: TranslateFn): string | null {
  if (summary.result === 'success') return null;
  if (!summary.failureStage) return t('routes.trace.failureUnknown');
  return t('routes.trace.stageFailed', { stage: activityTraceFailureStageLabel(summary.failureStage, t) });
}

function activityTraceFailureStageLabel(stage: ActivityTraceFailureStageId, t: TranslateFn): string {
  if (stage === 'local_endpoint') return t('routes.trace.stageId.local_endpoint');
  if (stage === 'admission') return t('routes.trace.detailStage.admission');
  if (stage === 'route_resolution') return t('routes.trace.detailStage.routeResolution');
  if (stage === 'response_conversion') return t('routes.trace.detailStage.responseConversion');
  if (stage === 'delivery') return t('routes.trace.detailStage.delivery');
  return activityTraceStageLabel(stage, t);
}
