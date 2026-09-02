import type { ReactNode } from 'react';
import { Tip } from '@/components/ui/tooltip';
import { useI18n } from '@/components/shared/LanguageProvider';
import type { RouteTraceListItem } from '@/components/shared/RouteTraceList';
import { Button } from '@/components/ui/button';
import {
  ColumnResizeHandle,
  Table,
  TableBody,
  TableCell,
  TableEmptyCell,
  TableHead,
  TableHeader,
  TableHeaderRow,
  TableRow,
  TableShell,
  useColumnWidths,
} from '@/components/ui/table';
import { formatInboundAt } from '@/pages/bridges/route-endpoint-copy';
import {
  ACTIVITY_TRACE_COLUMN_WIDTHS_STORAGE_KEY,
  ACTIVITY_TRACE_STAGES,
  ACTIVITY_TRACE_WIDTH_SPECS,
  activityTraceColumnLabel,
  activityTraceModelLabel,
  activityTraceStageLabel,
  activityTraceStageStatusLabel,
  formatTraceSeconds,
  formatTraceTokens,
  type ActivityTraceColumnKey,
} from './activity-trace-list-model';
import {
  activityTraceResultLabel,
  activityTraceStageStatus,
  summarizeActivityTrace,
} from './activity-trace-summary-model';
import { ActivityTraceStageIcon } from './ActivityTraceStageDisplay';

export function ActivityTraceList({
  rows,
  emptyLabel,
  activeId,
  onShowDetail,
}: {
  rows: readonly RouteTraceListItem[];
  emptyLabel?: string;
  activeId?: string | null;
  onShowDetail?: (row: RouteTraceListItem) => void;
}) {
  const { t } = useI18n();
  const { widths, onResizeStart, totalWidth } = useColumnWidths(
    ACTIVITY_TRACE_WIDTH_SPECS,
    ACTIVITY_TRACE_COLUMN_WIDTHS_STORAGE_KEY,
  );

  return (
    <TableShell layout="split">
      <Table className="table-fixed" style={{ minWidth: totalWidth }}>
        <colgroup>
          {ACTIVITY_TRACE_WIDTH_SPECS.map((spec) => (
            <col key={spec.key} style={{ width: widths[spec.key] }} />
          ))}
        </colgroup>
        <TableHeader>
          <TableHeaderRow>
            {ACTIVITY_TRACE_WIDTH_SPECS.map((spec) => {
              const label = activityTraceColumnLabel(spec.key, t);
              return (
                <TableHead key={spec.key} className="relative select-none" data-col={spec.key}>
                  {label}
                  <ColumnResizeHandle
                    columnKey={spec.key}
                    label={label}
                    onResizeStart={onResizeStart}
                  />
                </TableHead>
              );
            })}
          </TableHeaderRow>
        </TableHeader>
        <TableBody>
          {rows.length === 0 ? (
            <TableRow>
              <TableCell colSpan={ACTIVITY_TRACE_WIDTH_SPECS.length} className="text-meta text-muted">
                {emptyLabel || <TableEmptyCell />}
              </TableCell>
            </TableRow>
          ) : (
            rows.map((row) => (
              <TableRow
                key={row.requestId}
                data-activity-trace-row={row.requestId}
                active={activeId === row.requestId}
                onOpen={onShowDetail ? () => onShowDetail(row) : undefined}
              >
                {ACTIVITY_TRACE_WIDTH_SPECS.map((spec) => (
                  <TableCell
                    key={spec.key}
                    data-col={spec.key}
                    className={spec.key === 'request' || spec.key === 'route' || spec.key === 'model'
                      ? 'min-w-0'
                      : 'whitespace-nowrap'}
                  >
                    {renderColumn(spec.key, row, {
                      t,
                      open: activeId === row.requestId,
                      onShowDetail,
                    })}
                  </TableCell>
                ))}
              </TableRow>
            ))
          )}
        </TableBody>
      </Table>
    </TableShell>
  );
}

function renderColumn(
  key: ActivityTraceColumnKey,
  row: RouteTraceListItem,
  ctx: {
    t: ReturnType<typeof useI18n>['t'];
    open: boolean;
    onShowDetail?: (row: RouteTraceListItem) => void;
  },
): ReactNode {
  const { t } = ctx;
  if (key === 'time') {
    return <span className="font-mono text-meta text-secondary">{formatInboundAt(row.at)}</span>;
  }
  if (key === 'request') {
    return (
      <div className="min-w-0">
        <p className="flex min-w-0 items-baseline gap-2 font-mono text-meta">
          <span className="shrink-0 text-secondary">{row.method}</span>
          <Tip label={row.path}>
            <span className="min-w-0 truncate text-primary">{row.path}</span>
          </Tip>
        </p>
      </div>
    );
  }
  if (key === 'model') {
    const model = activityTraceModelLabel(row);
    if (!model) return <TableEmptyCell />;
    return (
      <Tip label={model}>
        <span className="block min-w-0 truncate font-mono text-meta text-secondary">{model}</span>
      </Tip>
    );
  }
  if (key === 'firstToken') {
    const value = formatTraceSeconds(row.ttftMs, t);
    return value ? <span className="font-mono text-meta text-muted">{value}</span> : <TableEmptyCell />;
  }
  if (key === 'duration') {
    const value = formatTraceSeconds(row.latencyMs, t);
    return value ? <span className="font-mono text-meta text-muted">{value}</span> : <TableEmptyCell />;
  }
  if (key === 'tokens') {
    const value = formatTraceTokens(row.inputTokens, row.outputTokens, t);
    return value ? <span className="font-mono text-meta text-muted">{value}</span> : <TableEmptyCell />;
  }
  if (key === 'stages') {
    const summary = summarizeActivityTrace(row);
    return (
      <div
        className="flex min-w-max items-center gap-1.5 whitespace-nowrap"
        aria-label={t('routes.trace.pipelineAria')}
        data-activity-trace-result={summary.result}
      >
        <span className={summary.result === 'success' ? 'shrink-0 text-meta text-success' : 'shrink-0 text-meta text-danger'}>
          {activityTraceResultLabel(summary, t)}
        </span>
        {ACTIVITY_TRACE_STAGES.map((stage) => {
          const status = activityTraceStageStatus(row, stage);
          const label = `${activityTraceStageLabel(stage, t)} · ${activityTraceStageStatusLabel(status, t)}`;
          return (
            <Tip key={stage} label={label}>
              <span
                className="inline-flex h-5 w-5 items-center justify-center"
                aria-label={label}
                data-stage={stage}
                data-stage-status={status}
              >
                <ActivityTraceStageIcon status={status} />
              </span>
            </Tip>
          );
        })}
      </div>
    );
  }
  if (key === 'route') {
    if (!row.sourceLabel) return <TableEmptyCell />;
    return (
      <Tip label={row.sourceLabel}>
        <span className="block min-w-0 truncate text-meta text-secondary">{row.sourceLabel}</span>
      </Tip>
    );
  }
  return (
    <Button
      type="button"
      variant="outline"
      size="sm"
      aria-label={t('routes.activity.openDetailAria')}
      aria-expanded={ctx.open}
      onClick={() => ctx.onShowDetail?.(row)}
    >
      {t('routes.activity.colDetails')}
    </Button>
  );
}
