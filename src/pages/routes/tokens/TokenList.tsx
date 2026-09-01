import type { KeyboardEvent, MouseEvent, ReactNode } from 'react';
import { Copy, Pencil } from 'lucide-react';
import { useI18n } from '@/components/shared/LanguageProvider';
import { CopyableRouteEndpointUrl, RouteEndpointUrl } from '@/components/shared/RouteEndpointUrl';
import { Button } from '@/components/ui/button';
import { useToast } from '@/components/ui/toast';
import {
  ColumnResizeHandle,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableHeaderRow,
  TableRow,
  TableShell,
  useColumnWidths,
  type ColumnWidthSpec,
} from '@/components/ui/table';
import { localEndpointBrandAgentId } from '@/lib/route-endpoints';
import { tokenEndpointParts } from './token-detail-model';
import { tokenTypeLabel, type LocalTokenRow } from './tokens-model';

type TokenColumnKey = 'type' | 'endpoint' | 'token';

const WIDTH_SPECS: ColumnWidthSpec<TokenColumnKey>[] = [
  { key: 'type', defaultWidth: 168, minWidth: 112 },
  { key: 'endpoint', defaultWidth: 360, minWidth: 200 },
  { key: 'token', defaultWidth: 240, minWidth: 160 },
];

const COLUMN_WIDTHS_STORAGE_KEY = 'agenthub.routes.tokens.columnWidths';

function isInteractiveTableTarget(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) return false;
  return Boolean(
    target.closest('button, a, input, textarea, [role="button"], [role="switch"], [role="menuitem"]'),
  );
}

function EmptyCell() {
  return <span className="text-muted">—</span>;
}

function columnLabel(key: TokenColumnKey, t: ReturnType<typeof useI18n>['t']): string {
  if (key === 'type') return t('routes.tokens.fieldType');
  if (key === 'endpoint') return t('routes.tokens.fieldEndpoint');
  return t('routes.tokens.fieldToken');
}

export function TokenList({
  rows,
  activeId,
  onShowDetail,
  onEditKey,
}: {
  rows: readonly LocalTokenRow[];
  activeId?: string | null;
  onShowDetail?: (row: LocalTokenRow) => void;
  onEditKey?: (row: LocalTokenRow) => void;
}) {
  const { t } = useI18n();
  const { toast } = useToast();
  const { widths, onResizeStart } = useColumnWidths(WIDTH_SPECS, COLUMN_WIDTHS_STORAGE_KEY);
  const totalWidth = WIDTH_SPECS.reduce((sum, spec) => sum + widths[spec.key], 0);

  return (
    <TableShell className="min-w-0 [&>div]:min-w-0 [&>div]:!overflow-x-scroll">
      <Table
        className="table-fixed"
        style={{ width: `max(100%, ${totalWidth}px)`, minWidth: totalWidth }}
      >
        <colgroup>
          {WIDTH_SPECS.map((spec) => (
            <col key={spec.key} style={{ width: widths[spec.key] }} />
          ))}
        </colgroup>
        <TableHeader>
          <TableHeaderRow>
            {WIDTH_SPECS.map((spec) => {
              const label = columnLabel(spec.key, t);
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
          {rows.map((row) => (
            <TableRow
              key={row.id}
              data-token-row={row.id}
              active={activeId === row.id}
              tabIndex={onShowDetail ? 0 : undefined}
              className={onShowDetail ? 'cursor-pointer' : undefined}
              onClick={onShowDetail ? (event: MouseEvent<HTMLTableRowElement>) => {
                if (event.defaultPrevented) return;
                if (isInteractiveTableTarget(event.target)) return;
                onShowDetail(row);
              } : undefined}
              onKeyDown={onShowDetail ? (event: KeyboardEvent<HTMLTableRowElement>) => {
                if (event.key !== 'Enter' && event.key !== ' ') return;
                if (isInteractiveTableTarget(event.target)) return;
                event.preventDefault();
                onShowDetail(row);
              } : undefined}
            >
              {WIDTH_SPECS.map((spec) => (
                <TableCell
                  key={spec.key}
                  data-col={spec.key}
                  className={spec.key === 'type' ? 'whitespace-nowrap' : 'min-w-0'}
                >
                  {renderColumn(spec.key, row, {
                    t,
                    toast,
                    onEditKey,
                  })}
                </TableCell>
              ))}
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </TableShell>
  );
}

function renderColumn(
  key: TokenColumnKey,
  row: LocalTokenRow,
  ctx: {
    t: ReturnType<typeof useI18n>['t'];
    toast: ReturnType<typeof useToast>['toast'];
    onEditKey?: (row: LocalTokenRow) => void;
  },
): ReactNode {
  const { t } = ctx;
  if (key === 'type') {
    return <span className="font-medium text-primary">{tokenTypeLabel(row, t)}</span>;
  }
  if (key === 'endpoint') {
    const endpoint = tokenEndpointParts(row);
    const brandAgentId = localEndpointBrandAgentId(row.kind);
    if (endpoint.portPending) {
      return (
        <RouteEndpointUrl
          path={row.path}
          port={null}
          host={endpoint.host}
          endpointId={endpoint.endpointId}
          brandAgentId={brandAgentId}
          className="text-meta"
        />
      );
    }
    return (
      <CopyableRouteEndpointUrl
        path={row.path}
        port={Number(endpoint.portLabel)}
        host={endpoint.host}
        endpointId={endpoint.endpointId}
        brandAgentId={brandAgentId}
        className="text-meta"
      />
    );
  }
  if (row.unavailable && !row.maskedToken) {
    return <span className="text-meta text-muted">{t('routes.runtime.unavailable')}</span>;
  }
  const copyKey = (event: { stopPropagation: () => void }) => {
    event.stopPropagation();
    const value = row.token?.trim();
    if (!value) return;
    void navigator.clipboard.writeText(value).then(
      () => ctx.toast({ title: t('routes.tokens.copied'), variant: 'success' }),
      () => ctx.toast({ title: t('routes.tokens.copyFailed'), variant: 'danger' }),
    );
  };
  return (
    <div className="flex min-w-0 items-center gap-1">
      {row.maskedToken ? (
        <span className="min-w-0 truncate font-mono text-meta text-secondary">
          {row.maskedToken}
        </span>
      ) : (
        <EmptyCell />
      )}
      {row.token ? (
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="h-7 w-7 shrink-0 px-0"
          onClick={copyKey}
          aria-label={t('routes.tokens.copy')}
        >
          <Copy className="h-3 w-3" aria-hidden />
        </Button>
      ) : null}
      {ctx.onEditKey ? (
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="h-7 shrink-0 px-1.5"
          onClick={(event) => {
            event.stopPropagation();
            ctx.onEditKey?.(row);
          }}
        >
          <Pencil className="h-3 w-3" aria-hidden />
          {t('routes.tokens.editKey')}
        </Button>
      ) : null}
    </div>
  );
}
