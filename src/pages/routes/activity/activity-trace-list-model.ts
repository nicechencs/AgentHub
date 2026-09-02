import type { TranslateFn } from '@/lib/i18n';
import type { RouteTraceStageStatus } from '@/lib/backend/contracts/adapter';
import type { ColumnWidthSpec } from '@/components/ui/table';

export type ActivityTraceColumnKey =
  | 'time'
  | 'request'
  | 'result'
  | 'stages'
  | 'route'
  | 'latency';

export const ACTIVITY_TRACE_WIDTH_SPECS: ColumnWidthSpec<ActivityTraceColumnKey>[] = [
  { key: 'time', defaultWidth: 168, minWidth: 120 },
  { key: 'request', defaultWidth: 280, minWidth: 180 },
  { key: 'result', defaultWidth: 168, minWidth: 120 },
  { key: 'stages', defaultWidth: 148, minWidth: 120 },
  { key: 'route', defaultWidth: 148, minWidth: 96 },
  { key: 'latency', defaultWidth: 88, minWidth: 72 },
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
  if (key === 'result') return t('routes.trace.result');
  if (key === 'stages') return t('routes.activity.colStages');
  if (key === 'route') return t('routes.activity.colRoute');
  return t('routes.activity.colLatency');
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
