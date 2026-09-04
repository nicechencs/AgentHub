import type { ReactNode } from 'react';
import { ChevronLeft, ChevronRight } from 'lucide-react';
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
  TableFooterBar,
  TableHead,
  TableHeader,
  TableHeaderRow,
  TableRow,
  TableShell,
  useColumnWidths,
} from '@/components/ui/table';
import {
  ACTIVITY_PAGE_SIZE,
  buildActivityPageItems,
} from './activity-query-model';
import { formatInboundAt } from '@/pages/routes/shared/route-endpoint-copy';
import {
  ACTIVITY_TRACE_COLUMN_WIDTHS_STORAGE_KEY,
  ACTIVITY_TRACE_STAGES,
  ACTIVITY_TRACE_WIDTH_SPECS,
  activityTraceColumnLabel,
  activityTraceHoverDetail,
  activityTraceInboundEndpoint,
  activityTraceInboundPath,
  activityTraceKeyParts,
  activityTraceModelLabel,
  activityTraceStageLabel,
  activityTraceStageStatusLabel,
  activityTraceUpstreamEndpoint,
  activityTraceUpstreamPath,
  formatTraceSeconds,
  formatTraceTokens,
  type ActivityTraceColumnKey,
  type ActivityTraceKeyToken,
} from './activity-trace-list-model';
import {
  activityTraceResultLabel,
  activityTraceStageStatus,
  summarizeActivityTrace,
} from './activity-trace-summary-model';
import { ActivityTraceStageIcon } from './ActivityTraceStageDisplay';

const SELECT_COL_WIDTH = 40;

export function ActivityTraceList({
  rows,
  tokens = [],
  emptyLabel,
  activeId,
  onShowDetail,
  selectedIds,
  onToggleRow,
  onTogglePage,
  page,
  total,
  pageSize = ACTIVITY_PAGE_SIZE,
  onPageChange,
}: {
  rows: readonly RouteTraceListItem[];
  tokens?: readonly ActivityTraceKeyToken[];
  emptyLabel?: string;
  activeId?: string | null;
  onShowDetail?: (row: RouteTraceListItem) => void;
  selectedIds?: ReadonlySet<string>;
  onToggleRow?: (id: string) => void;
  onTogglePage?: () => void;
  page?: number;
  total?: number;
  pageSize?: number;
  onPageChange?: (next: number) => void;
}) {
  const { t } = useI18n();
  const { widths, onResizeStart, totalWidth } = useColumnWidths(
    ACTIVITY_TRACE_WIDTH_SPECS,
    ACTIVITY_TRACE_COLUMN_WIDTHS_STORAGE_KEY,
  );
  const selectable = Boolean(onToggleRow);
  const colCount = ACTIVITY_TRACE_WIDTH_SPECS.length + (selectable ? 1 : 0);
  const pageIds = rows.map((row) => row.requestId);
  const allPageSelected = pageIds.length > 0 && pageIds.every((id) => selectedIds?.has(id));
  const totalCount = total ?? rows.length;
  const totalPages = Math.max(1, Math.ceil(Math.max(0, totalCount) / pageSize));
  const safePage = Math.min(Math.max(1, page ?? 1), totalPages);
  const pageItems = buildActivityPageItems(safePage, totalPages);
  const showPager = Boolean(onPageChange) && totalCount > 0;

  return (
    <TableShell
      layout="split"
      footer={showPager ? (
        <TableFooterBar>
          <p>
            {t('routes.activity.total', { n: totalCount.toLocaleString() })}
            {t('routes.activity.page', { page: safePage, pages: totalPages })}
            <span className="text-muted/80">
              {t('routes.activity.pageSize', { n: pageSize })}
            </span>
          </p>
          {totalPages > 1 ? (
            <div className="flex flex-wrap items-center gap-1">
              <Button
                type="button"
                variant="outline"
                size="sm"
                disabled={safePage <= 1}
                aria-label={t('routes.activity.prevPage')}
                onClick={() => onPageChange?.(Math.max(1, safePage - 1))}
              >
                <ChevronLeft className="h-3.5 w-3.5" />
              </Button>
              {pageItems.map((item, idx) =>
                item === 'ellipsis' ? (
                  <span key={`e-${idx}`} className="px-1 text-xs text-muted" aria-hidden>
                    …
                  </span>
                ) : (
                  <Button
                    key={item}
                    type="button"
                    variant={item === safePage ? 'default' : 'outline'}
                    size="sm"
                    aria-label={t('routes.activity.pageN', { n: item })}
                    aria-current={item === safePage ? 'page' : undefined}
                    className="min-w-7 px-2"
                    onClick={() => onPageChange?.(item)}
                  >
                    {item}
                  </Button>
                ),
              )}
              <Button
                type="button"
                variant="outline"
                size="sm"
                disabled={safePage >= totalPages}
                aria-label={t('routes.activity.nextPage')}
                onClick={() => onPageChange?.(Math.min(totalPages, safePage + 1))}
              >
                <ChevronRight className="h-3.5 w-3.5" />
              </Button>
            </div>
          ) : null}
        </TableFooterBar>
      ) : undefined}
    >
      <Table className="table-fixed" style={{ minWidth: totalWidth + (selectable ? SELECT_COL_WIDTH : 0) }}>
        <colgroup>
          {selectable ? <col style={{ width: SELECT_COL_WIDTH }} /> : null}
          {ACTIVITY_TRACE_WIDTH_SPECS.map((spec) => (
            <col key={spec.key} style={{ width: widths[spec.key] }} />
          ))}
        </colgroup>
        <TableHeader>
          <TableHeaderRow>
            {selectable ? (
              <TableHead data-col="select" className="w-10">
                <input
                  type="checkbox"
                  className="h-3.5 w-3.5 accent-accent"
                  checked={allPageSelected}
                  disabled={pageIds.length === 0}
                  onChange={() => onTogglePage?.()}
                  aria-label={t('routes.activity.selectPageAria')}
                />
              </TableHead>
            ) : null}
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
              <TableCell colSpan={colCount} className="text-meta text-muted">
                {emptyLabel || <TableEmptyCell />}
              </TableCell>
            </TableRow>
          ) : (
            rows.map((row) => (
              <TableRow
                key={row.requestId}
                data-activity-trace-row={row.requestId}
                active={activeId === row.requestId}
                selected={selectedIds?.has(row.requestId)}
                onOpen={onShowDetail ? () => onShowDetail(row) : undefined}
              >
                {selectable ? (
                  <TableCell data-col="select" className="w-10">
                    <input
                      type="checkbox"
                      className="h-3.5 w-3.5 accent-accent"
                      checked={selectedIds?.has(row.requestId) === true}
                      onChange={() => onToggleRow?.(row.requestId)}
                      aria-label={t('routes.activity.selectRowAria')}
                    />
                  </TableCell>
                ) : null}
                {ACTIVITY_TRACE_WIDTH_SPECS.map((spec) => (
                  <TableCell
                    key={spec.key}
                    data-col={spec.key}
                    className={spec.key === 'key' || spec.key === 'endpoint' || spec.key === 'route' || spec.key === 'model'
                      ? 'min-w-0'
                      : 'whitespace-nowrap'}
                  >
                    {renderColumn(spec.key, row, {
                      t,
                      tokens,
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

function HoverDetail({ title, value }: { title: string; value: string }) {
  return (
    <span className="block max-w-[22rem]">
      <span className="block text-muted">{title}</span>
      <span className="mt-0.5 block font-mono">{value}</span>
    </span>
  );
}

function EndpointLine({
  mark,
  path,
  title,
  full,
  dir,
}: {
  mark: string;
  path: string;
  title: string;
  full: string;
  dir: 'in' | 'out';
}) {
  return (
    <Tip label={full ? <HoverDetail title={`${mark} · ${title}`} value={full} /> : undefined}>
      <span
        className="flex min-w-0 items-baseline gap-1.5"
        data-endpoint-dir={dir}
        aria-label={full ? activityTraceHoverDetail(`${mark} · ${title}`, full) : mark}
      >
        <span className="w-8 shrink-0 text-caption text-muted">{mark}</span>
        <span className={dir === 'in'
          ? 'min-w-0 truncate font-mono text-meta text-primary'
          : 'min-w-0 truncate font-mono text-meta text-muted'}
        >
          {path || '—'}
        </span>
      </span>
    </Tip>
  );
}

function renderColumn(
  key: ActivityTraceColumnKey,
  row: RouteTraceListItem,
  ctx: {
    t: ReturnType<typeof useI18n>['t'];
    tokens: readonly ActivityTraceKeyToken[];
    open: boolean;
    onShowDetail?: (row: RouteTraceListItem) => void;
  },
): ReactNode {
  const { t } = ctx;
  if (key === 'time') {
    return <span className="font-mono text-meta text-secondary">{formatInboundAt(row.at)}</span>;
  }
  if (key === 'key') {
    const keyParts = activityTraceKeyParts(row, ctx.tokens);
    if (!keyParts.label) return <TableEmptyCell />;
    return (
      <Tip label={<HoverDetail title={t('routes.activity.localKey')} value={keyParts.label} />}>
        <span
          className="block min-w-0 truncate text-meta"
          aria-label={activityTraceHoverDetail(t('routes.activity.localKey'), keyParts.label)}
        >
          {keyParts.abbrev ? (
            <span className="font-mono text-secondary">{keyParts.abbrev}</span>
          ) : null}
          {keyParts.abbrev && keyParts.name ? ' ' : null}
          {keyParts.name ? <span className="text-primary">{keyParts.name}</span> : null}
        </span>
      </Tip>
    );
  }
  if (key === 'endpoint') {
    const inbound = activityTraceInboundPath(row);
    const upstream = activityTraceUpstreamPath(row);
    if (!inbound && !upstream) return <TableEmptyCell />;
    return (
      <div className="min-w-0 leading-tight">
        <EndpointLine
          mark={t('routes.activity.inboundMark')}
          path={inbound}
          title={t('routes.activity.inboundEndpoint')}
          full={activityTraceInboundEndpoint(row)}
          dir="in"
        />
        <EndpointLine
          mark={t('routes.activity.outboundMark')}
          path={upstream}
          title={t('routes.activity.outboundEndpoint')}
          full={activityTraceUpstreamEndpoint(row)}
          dir="out"
        />
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
