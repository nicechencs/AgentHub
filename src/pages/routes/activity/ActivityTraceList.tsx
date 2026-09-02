import { Fragment, useState, type ReactNode } from 'react';
import { Check, Minus, X } from 'lucide-react';
import { Tip } from '@/components/ui/tooltip';
import { useI18n } from '@/components/shared/LanguageProvider';
import { RouteTraceFlowDiagram } from '@/components/shared/RouteTraceFlowDiagram';
import type { RouteTraceListItem } from '@/components/shared/RouteTraceList';
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
import type { RouteTraceStageStatus } from '@/lib/backend/contracts/adapter';
import { formatInboundAt } from '@/pages/bridges/route-endpoint-copy';
import {
  ACTIVITY_TRACE_COLUMN_WIDTHS_STORAGE_KEY,
  ACTIVITY_TRACE_STAGES,
  ACTIVITY_TRACE_WIDTH_SPECS,
  activityTraceColumnLabel,
  activityTraceStageLabel,
  activityTraceStageStatusLabel,
  type ActivityTraceColumnKey,
  type ActivityTraceStageId,
} from './activity-trace-list-model';

function stageTone(status: RouteTraceStageStatus): string {
  if (status === 'ok') return 'text-success';
  if (status === 'failed') return 'text-danger';
  if (status === 'skipped') return 'text-muted';
  return 'text-secondary';
}

function StageIcon({ status }: { status: RouteTraceStageStatus }) {
  if (status === 'ok') return <Check className="h-3.5 w-3.5 text-success" aria-hidden />;
  if (status === 'failed') return <X className="h-3.5 w-3.5 text-danger" aria-hidden />;
  if (status === 'skipped') return <Minus className="h-3.5 w-3.5 text-muted" aria-hidden />;
  return <span className="inline-block h-2 w-2 rounded-full bg-muted" aria-hidden />;
}

function stageStatusOf(
  row: RouteTraceListItem,
  stage: ActivityTraceStageId,
): RouteTraceStageStatus {
  if (stage === 'local_auth') return row.localAuth.status;
  if (stage === 'pool') return row.pool.status;
  if (stage === 'conversion') return row.conversion.status;
  if (stage === 'upstream_auth') return row.upstreamAuth.status;
  return row.upstream.status;
}

function failureStageLabel(
  stage: string | null | undefined,
  t: ReturnType<typeof useI18n>['t'],
): string | null {
  if (!stage) return null;
  if (
    stage === 'local_auth'
    || stage === 'pool'
    || stage === 'conversion'
    || stage === 'upstream_auth'
    || stage === 'upstream'
  ) {
    return activityTraceStageLabel(stage, t);
  }
  return stage;
}

export function ActivityTraceList({
  rows,
  emptyLabel,
}: {
  rows: readonly RouteTraceListItem[];
  emptyLabel?: string;
}) {
  const { t } = useI18n();
  const { widths, onResizeStart, totalWidth } = useColumnWidths(
    ACTIVITY_TRACE_WIDTH_SPECS,
    ACTIVITY_TRACE_COLUMN_WIDTHS_STORAGE_KEY,
  );
  const [openId, setOpenId] = useState<string | null>(null);

  return (
    <TableShell layout="page">
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
            rows.map((row) => {
              const open = openId === row.requestId;
              return (
                <Fragment key={row.requestId}>
                  <TableRow
                    data-activity-trace-row={row.requestId}
                    active={open}
                    onOpen={() => setOpenId(open ? null : row.requestId)}
                  >
                    {ACTIVITY_TRACE_WIDTH_SPECS.map((spec) => (
                      <TableCell
                        key={spec.key}
                        data-col={spec.key}
                        className={spec.key === 'request' || spec.key === 'route' ? 'min-w-0' : 'whitespace-nowrap'}
                      >
                        {renderColumn(spec.key, row, t)}
                      </TableCell>
                    ))}
                  </TableRow>
                  {open ? (
                    <TableRow data-activity-trace-detail={row.requestId}>
                      <TableCell colSpan={ACTIVITY_TRACE_WIDTH_SPECS.length} className="bg-subtle/40">
                        <RouteTraceFlowDiagram row={row} />
                        <TraceExtraLines row={row} />
                      </TableCell>
                    </TableRow>
                  ) : null}
                </Fragment>
              );
            })
          )}
        </TableBody>
      </Table>
    </TableShell>
  );
}

function renderColumn(
  key: ActivityTraceColumnKey,
  row: RouteTraceListItem,
  t: ReturnType<typeof useI18n>['t'],
): ReactNode {
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
        {row.model ? (
          <p className="truncate text-meta text-muted">{row.model}</p>
        ) : null}
      </div>
    );
  }
  if (key === 'result') {
    const failedAt = failureStageLabel(row.failureStage, t);
    return (
      <div className="min-w-0">
        <p className="flex items-baseline gap-2 text-meta">
          <span className={row.ok ? 'text-success' : 'text-danger'}>
            {row.ok ? t('routes.inbound.ok') : t('routes.inbound.fail')}
          </span>
          <span className={stageTone(row.ok ? 'ok' : 'failed')}>{row.httpStatus}</span>
        </p>
        {failedAt ? (
          <p className="truncate text-meta text-danger">
            {t('routes.trace.failedAt', { stage: failedAt })}
          </p>
        ) : null}
      </div>
    );
  }
  if (key === 'stages') {
    return (
      <div className="flex items-center gap-1" aria-label={t('routes.trace.pipelineAria')}>
        {ACTIVITY_TRACE_STAGES.map((stage) => {
          const status = stageStatusOf(row, stage);
          const label = `${activityTraceStageLabel(stage, t)} · ${activityTraceStageStatusLabel(status, t)}`;
          return (
            <Tip key={stage} label={label}>
              <span
                className="inline-flex h-5 w-5 items-center justify-center"
                aria-label={label}
                data-stage={stage}
                data-stage-status={status}
              >
                <StageIcon status={status} />
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
  if (row.latencyMs == null) return <TableEmptyCell />;
  return (
    <span className="font-mono text-meta text-muted">
      {t('routes.trace.latencyMs', { ms: row.latencyMs })}
    </span>
  );
}

function TraceExtraLines({ row }: { row: RouteTraceListItem }) {
  const { t } = useI18n();
  const lines: string[] = [];
  if (row.pool.attempts?.length) {
    for (const [index, attempt] of row.pool.attempts.entries()) {
      lines.push(
        `${t('routes.trace.attempt', { n: index + 1 })}: ${attempt.member.label} · ${attempt.status}${attempt.code ? ` (${attempt.code})` : ''}`,
      );
    }
  }
  if (row.conversion.message) lines.push(row.conversion.message);
  if (row.upstream.message) lines.push(row.upstream.message);
  if (lines.length === 0) return null;
  return (
    <ul className="mt-2 space-y-0.5 border-t border-border pt-2 text-meta text-muted">
      {lines.map((line) => (
        <li key={line} className="truncate">{line}</li>
      ))}
    </ul>
  );
}
