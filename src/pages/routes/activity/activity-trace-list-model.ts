import type { TranslateFn } from '@/lib/i18n';
import type { RouteTraceStageStatus } from '@/lib/backend/contracts/adapter';
import type { ColumnWidthSpec } from '@/components/ui/table';
import { fmtTokens } from '@/lib/utils';

export type ActivityTraceColumnKey =
  | 'time'
  | 'request'
  | 'model'
  | 'firstToken'
  | 'duration'
  | 'tokens'
  | 'stages'
  | 'route'
  | 'details';

export const ACTIVITY_TRACE_WIDTH_SPECS: ColumnWidthSpec<ActivityTraceColumnKey>[] = [
  { key: 'time', defaultWidth: 148, minWidth: 112 },
  { key: 'request', defaultWidth: 220, minWidth: 148 },
  { key: 'model', defaultWidth: 140, minWidth: 96 },
  { key: 'firstToken', defaultWidth: 80, minWidth: 64 },
  { key: 'duration', defaultWidth: 96, minWidth: 72 },
  { key: 'tokens', defaultWidth: 120, minWidth: 88 },
  { key: 'stages', defaultWidth: 132, minWidth: 112 },
  { key: 'route', defaultWidth: 120, minWidth: 88 },
  { key: 'details', defaultWidth: 72, minWidth: 64 },
];

export const ACTIVITY_TRACE_COLUMN_WIDTHS_STORAGE_KEY =
  'agenthub.routes.activity.columnWidths';

export const ACTIVITY_TRACE_STAGES = [
  'local_auth',
  'pool',
  'conversion',
  'upstream_auth',
  'upstream',
] as const;

export type ActivityTraceStageId = (typeof ACTIVITY_TRACE_STAGES)[number];

export function activityTraceColumnLabel(
  key: ActivityTraceColumnKey,
  t: TranslateFn,
): string {
  if (key === 'time') return t('routes.activity.colTime');
  if (key === 'request') return t('routes.activity.colRequest');
  if (key === 'model') return t('routes.activity.colModel');
  if (key === 'firstToken') return t('routes.activity.colFirstToken');
  if (key === 'duration') return t('routes.activity.colDuration');
  if (key === 'tokens') return t('routes.activity.colTokens');
  if (key === 'stages') return t('routes.activity.colStages');
  if (key === 'route') return t('routes.activity.colRoute');
  return t('routes.activity.colDetails');
}

export function activityTraceStageLabel(stage: ActivityTraceStageId, t: TranslateFn): string {
  switch (stage) {
    case 'local_auth':
      return t('routes.trace.stageId.local_auth');
    case 'pool':
      return t('routes.trace.stageId.pool');
    case 'conversion':
      return t('routes.trace.stageId.conversion');
    case 'upstream_auth':
      return t('routes.trace.stageId.upstream_auth');
    default:
      return t('routes.trace.stageId.upstream');
  }
}

export function activityTraceStageStatusLabel(
  status: RouteTraceStageStatus,
  t: TranslateFn,
): string {
  if (status === 'ok') return t('routes.inbound.ok');
  if (status === 'failed') return t('routes.inbound.fail');
  if (status === 'skipped') return t('routes.trace.flow.stageSkipped');
  return t('routes.trace.flow.authPending');
}

export function formatTraceSeconds(
  ms: number | null | undefined,
  t: TranslateFn,
): string {
  if (ms == null) return '';
  const seconds = ms / 1000;
  const label = seconds < 10 ? seconds.toFixed(1) : String(Math.round(seconds));
  return t('routes.activity.seconds', { s: label });
}

export function formatTraceTokens(
  inputTokens: number | null | undefined,
  outputTokens: number | null | undefined,
  t: TranslateFn,
): string {
  if (inputTokens == null && outputTokens == null) return '';
  return t('routes.activity.tokensValue', {
    in: fmtTokens(inputTokens ?? 0),
    out: fmtTokens(outputTokens ?? 0),
  });
}

export function activityTraceModelLabel(row: {
  model?: string | null;
  upstream?: { model?: string | null; upstreamModel?: string | null };
}): string {
  return row.model?.trim()
    || row.upstream?.upstreamModel?.trim()
    || row.upstream?.model?.trim()
    || '';
}
