import type { ReactNode } from 'react';
import { Copy, Trash2 } from 'lucide-react';
import { useI18n } from '@/components/shared/LanguageProvider';
import { CopyableRouteEndpointUrl, RouteEndpointUrl } from '@/components/shared/RouteEndpointUrl';
import { Button } from '@/components/ui/button';
import { useToast } from '@/components/ui/toast';
import { Hint, Tip } from '@/components/ui/tooltip';
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
  type ColumnWidthSpec,
} from '@/components/ui/table';
import { localEndpointBrandAgentId } from '@/lib/route-endpoints';
import {
  formatTokenRelative,
  tokenEndpointParts,
  tokenLastPageDisplay,
  tokenUsageDisplay,
} from './token-detail-model';
import { tokenDisplayName, tokenTypeLabel, type LocalTokenRow } from './tokens-model';
import { TokenImportToAgentButton } from './TokenImportToAgentButton';
import type { TokenImportAgentRef } from './token-import-model';
import { StorageKey } from '@/lib/ui-preferences';

type TokenColumnKey = 'name' | 'type' | 'endpoint' | 'token' | 'lastPage' | 'usage';

const WIDTH_SPECS: ColumnWidthSpec<TokenColumnKey>[] = [
  { key: 'name', defaultWidth: 160, minWidth: 96 },
  { key: 'type', defaultWidth: 168, minWidth: 112 },
  { key: 'endpoint', defaultWidth: 280, minWidth: 180 },
  { key: 'token', defaultWidth: 280, minWidth: 180 },
  { key: 'lastPage', defaultWidth: 180, minWidth: 120 },
  { key: 'usage', defaultWidth: 148, minWidth: 112 },
];

const COLUMN_WIDTHS_STORAGE_KEY = StorageKey.routesTokensColumnWidths;

function columnLabel(key: TokenColumnKey, t: ReturnType<typeof useI18n>['t']): string {
  if (key === 'name') return t('routes.tokens.fieldName');
  if (key === 'type') return t('routes.tokens.fieldType');
  if (key === 'endpoint') return t('routes.tokens.fieldEndpoint');
  if (key === 'lastPage') return t('routes.tokens.fieldLastPage');
  if (key === 'usage') return t('routes.tokens.fieldUsage');
  return t('routes.tokens.fieldToken');
}

export function TokenList({
  rows,
  activeId,
  onShowDetail,
  onDelete,
  installedAgents,
}: {
  rows: readonly LocalTokenRow[];
  activeId?: string | null;
  onShowDetail?: (row: LocalTokenRow) => void;
  onDelete?: (row: LocalTokenRow) => void;
  installedAgents?: readonly TokenImportAgentRef[];
}) {
  const { t } = useI18n();
  const { toast } = useToast();
  const { widths, onResizeStart, totalWidth } = useColumnWidths(
    WIDTH_SPECS,
    COLUMN_WIDTHS_STORAGE_KEY,
  );

  return (
    <TableShell layout="split">
      <Table className="table-fixed" style={{ minWidth: totalWidth }}>
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
              onOpen={onShowDetail ? () => onShowDetail(row) : undefined}
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
                    installedAgents,
                    onDelete,
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
    installedAgents?: readonly TokenImportAgentRef[];
    onDelete?: (row: LocalTokenRow) => void;
  },
): ReactNode {
  const { t } = ctx;
  if (key === 'name') {
    const label = tokenDisplayName(row, t);
    return (
      <Tip className="block truncate font-medium text-primary" label={label}>
        {label}
      </Tip>
    );
  }
  if (key === 'type') {
    return <span className="block truncate font-medium text-primary">{tokenTypeLabel(row, t)}</span>;
  }
  if (key === 'lastPage') {
    const page = tokenLastPageDisplay(row);
    const at = formatTokenRelative(row.lastRequestAt, t);
    if (!page && !at) return <TableEmptyCell />;
    return (
      <div className="min-w-0">
        <p className="truncate font-mono text-meta text-secondary">{page || '—'}</p>
        {at ? <p className="truncate text-meta text-muted">{at}</p> : null}
      </div>
    );
  }
  if (key === 'usage') {
    return tokenUsageDisplay(row.usage, t) || <TableEmptyCell />;
  }
  if (key === 'endpoint') {
    const endpoint = tokenEndpointParts(row);
    const brandAgentId = localEndpointBrandAgentId(row.kind);
    const url = endpoint.portPending ? (
      <RouteEndpointUrl
        path={row.path}
        port={null}
        host={endpoint.host}
        endpointId={endpoint.endpointId}
        brandAgentId={brandAgentId}
        className="text-meta"
      />
    ) : (
      <CopyableRouteEndpointUrl
        path={row.path}
        port={Number(endpoint.portLabel)}
        host={endpoint.host}
        endpointId={endpoint.endpointId}
        brandAgentId={brandAgentId}
        className="text-meta"
      />
    );
    return <div className="min-w-0 max-w-full">{url}</div>;
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
        <TableEmptyCell />
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
      {ctx.installedAgents ? (
        <TokenImportToAgentButton
          row={row}
          installedAgents={ctx.installedAgents}
          className="shrink-0"
        />
      ) : null}
      {ctx.onDelete && row.canDelete ? (
        <Hint label={t('routes.tokens.delete')}>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="h-7 w-7 shrink-0 px-0"
            onClick={(event) => {
              event.stopPropagation();
              ctx.onDelete?.(row);
            }}
            aria-label={t('routes.tokens.delete')}
          >
            <Trash2 className="h-3 w-3" aria-hidden />
          </Button>
        </Hint>
      ) : null}
    </div>
  );
}
