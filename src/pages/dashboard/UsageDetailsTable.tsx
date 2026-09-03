import { useEffect, useMemo, useState } from 'react';
import { ChevronLeft, ChevronRight } from 'lucide-react';

import { AgentDot } from '@/components/shared/AgentDot';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
import {
  ColumnResizeHandle,
  Table,
  TableBody,
  TableCell,
  TableFooterBar,
  TableHead,
  TableHeader,
  TableHeaderRow,
  TableRow,
  TableShell,
  useColumnWidths,
  type ColumnWidthSpec,
} from '@/components/ui/table';
import { Tip } from '@/components/ui/tooltip';
import { agentDisplayName } from '@/config/agents';
import type { MessageKey } from '@/lib/i18n';
import type { UsageRecord } from '@/lib/types';
import { cn } from '@/lib/utils';
import { StorageKey } from '@/lib/ui-preferences';

const PAGE_SIZE = 50;

type ColumnKey =
  | 'timestamp'
  | 'agent'
  | 'model'
  | 'input'
  | 'output'
  | 'cacheWrite'
  | 'cacheRead'
  | 'cost'
  | 'session';

const COLUMNS: (ColumnWidthSpec<ColumnKey> & {
  align: 'left' | 'right';
})[] = [
  { key: 'timestamp', align: 'left', defaultWidth: 120, minWidth: 88 },
  { key: 'agent', align: 'left', defaultWidth: 110, minWidth: 80 },
  { key: 'model', align: 'left', defaultWidth: 160, minWidth: 96 },
  { key: 'input', align: 'right', defaultWidth: 96, minWidth: 72 },
  { key: 'output', align: 'right', defaultWidth: 96, minWidth: 72 },
  { key: 'cacheWrite', align: 'right', defaultWidth: 96, minWidth: 72 },
  { key: 'cacheRead', align: 'right', defaultWidth: 96, minWidth: 72 },
  { key: 'cost', align: 'right', defaultWidth: 88, minWidth: 64 },
  { key: 'session', align: 'left', defaultWidth: 180, minWidth: 96 },
];

const COLUMN_LABEL_KEYS: Record<ColumnKey, MessageKey> = {
  timestamp: 'dashboard.table.timestamp',
  agent: 'dashboard.table.agent',
  model: 'dashboard.table.model',
  input: 'dashboard.table.input',
  output: 'dashboard.table.output',
  cacheWrite: 'dashboard.table.cacheWrite',
  cacheRead: 'dashboard.table.cacheRead',
  cost: 'dashboard.table.cost',
  session: 'dashboard.table.session',
};

const WIDTH_SPECS: ColumnWidthSpec<ColumnKey>[] = COLUMNS.map(
  ({ key, defaultWidth, minWidth }) => ({ key, defaultWidth, minWidth }),
);

const COLUMN_WIDTHS_STORAGE_KEY = StorageKey.dashboardUsageColumnWidths;

function fmtTime(iso: string): string {
  const d = new Date(iso);
  const p = (n: number) => String(n).padStart(2, '0');
  return `${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}

/** 生成分页页码列表（含省略号），便于单测与 UI 复用 */
export function buildPageItems(current: number, total: number): Array<number | 'ellipsis'> {
  if (total <= 0) return [];
  if (total <= 7) {
    return Array.from({ length: total }, (_, i) => i + 1);
  }

  const items: Array<number | 'ellipsis'> = [1];
  const left = Math.max(2, current - 1);
  const right = Math.min(total - 1, current + 1);

  if (left > 2) items.push('ellipsis');
  for (let p = left; p <= right; p += 1) items.push(p);
  if (right < total - 1) items.push('ellipsis');
  items.push(total);
  return items;
}

export function UsageDetailsTable({ rows }: { rows: UsageRecord[] }) {
  const { t } = useI18n();
  const { widths, onResizeStart, totalWidth } = useColumnWidths(
    WIDTH_SPECS,
    COLUMN_WIDTHS_STORAGE_KEY,
  );
  const [page, setPage] = useState(1);

  const total = rows.length;
  const totalPages = Math.max(1, Math.ceil(total / PAGE_SIZE));
  const safePage = Math.min(page, totalPages);

  // 筛选/数据变化时回到第 1 页
  useEffect(() => {
    setPage(1);
  }, [rows]);

  const pageRows = useMemo(() => {
    const start = (safePage - 1) * PAGE_SIZE;
    return rows.slice(start, start + PAGE_SIZE);
  }, [rows, safePage]);

  const pageItems = useMemo(
    () => buildPageItems(safePage, totalPages),
    [safePage, totalPages],
  );

  return (
    <TableShell
      footer={
        <TableFooterBar>
          <p>
            {t('dashboard.table.total', { n: total.toLocaleString() })}
            {total > 0 && (
              <>
                {t('dashboard.table.page', { page: safePage, pages: totalPages })}
                <span className="text-muted/80">
                  {t('dashboard.table.pageSize', { n: PAGE_SIZE })}
                </span>
              </>
            )}
          </p>

          {totalPages > 1 && (
            <div className="flex flex-wrap items-center gap-1">
              <Button
                type="button"
                variant="outline"
                size="sm"
                disabled={safePage <= 1}
                aria-label={t('dashboard.table.prevPage')}
                onClick={() => setPage((p) => Math.max(1, p - 1))}
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
                    aria-label={t('dashboard.table.pageN', { n: item })}
                    aria-current={item === safePage ? 'page' : undefined}
                    className="min-w-7 px-2"
                    onClick={() => setPage(item)}
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
                aria-label={t('dashboard.table.nextPage')}
                onClick={() => setPage((p) => Math.min(totalPages, p + 1))}
              >
                <ChevronRight className="h-3.5 w-3.5" />
              </Button>
            </div>
          )}
        </TableFooterBar>
      }
    >
      <Table className="table-fixed" style={{ minWidth: totalWidth }}>
        <colgroup>
          {COLUMNS.map((c) => (
            <col key={c.key} style={{ width: widths[c.key] }} />
          ))}
        </colgroup>
        <TableHeader>
          <TableHeaderRow>
            {COLUMNS.map((c) => {
              const label = t(COLUMN_LABEL_KEYS[c.key]);
              return (
                <TableHead
                  key={c.key}
                  className={cn('relative select-none', c.align === 'right' && 'text-right')}
                >
                  {label}
                  <ColumnResizeHandle
                    columnKey={c.key}
                    label={label}
                    onResizeStart={onResizeStart}
                  />
                </TableHead>
              );
            })}
          </TableHeaderRow>
        </TableHeader>
        <TableBody>
          {pageRows.map((r) => (
            <TableRow key={r.id}>
              <TableCell className="truncate whitespace-nowrap font-mono text-xs text-secondary">
                {fmtTime(r.timestamp)}
              </TableCell>
              <TableCell className="truncate whitespace-nowrap">
                <span className="inline-flex max-w-full items-center gap-1.5">
                  <AgentDot agentId={r.agentId} />
                  <span className="truncate">{agentDisplayName(r.agentId)}</span>
                </span>
              </TableCell>
              <TableCell className="truncate whitespace-nowrap font-mono text-xs">
                {r.model}
              </TableCell>
              <TableCell className="truncate whitespace-nowrap text-right font-mono text-xs">
                {r.inputTokens.toLocaleString()}
              </TableCell>
              <TableCell className="truncate whitespace-nowrap text-right font-mono text-xs">
                {r.outputTokens.toLocaleString()}
              </TableCell>
              <TableCell className="truncate whitespace-nowrap text-right font-mono text-xs">
                {r.cacheWriteTokens.toLocaleString()}
              </TableCell>
              <TableCell className="truncate whitespace-nowrap text-right font-mono text-xs">
                {r.cacheReadTokens.toLocaleString()}
              </TableCell>
              <TableCell className="truncate whitespace-nowrap text-right font-mono text-xs">
                ${r.costUsd.toFixed(2)}
              </TableCell>
              <TableCell className="truncate whitespace-nowrap font-mono text-xs text-muted">
                <Tip className="block truncate" label={r.sessionId}>
                  {r.sessionId}
                </Tip>
              </TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </TableShell>
  );
}
