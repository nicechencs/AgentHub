import { Fragment, type ReactNode } from 'react';
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
import { localEndpointBrandAgentId, type LocalEndpointKind } from '@/lib/route-endpoints';
import { tokenEndpointParts } from './token-detail-model';
import {
  buildLocalTokenGroups,
  localTokenDeleteGate,
  localTokenEmptyCreateGate,
  tokenDisplayName,
  tokenTypeLabel,
  type LocalTokenRow,
} from './tokens-model';
import { TokenImportToAgentButton } from './TokenImportToAgentButton';
import type { TokenImportAgentRef } from './token-import-model';
import type { ConnectApiKeyDraft } from '@/lib/connect-flow/connect-intent';
import type { AgentKey } from '@/lib/types';
import { StorageKey } from '@/lib/ui-preferences';

type TokenColumnKey = 'name' | 'token';

const WIDTH_SPECS: ColumnWidthSpec<TokenColumnKey>[] = [
  { key: 'name', defaultWidth: 200, minWidth: 96 },
  { key: 'token', defaultWidth: 360, minWidth: 180 },
];

const COLUMN_WIDTHS_STORAGE_KEY = StorageKey.routesTokensColumnWidths;

function columnLabel(key: TokenColumnKey, t: ReturnType<typeof useI18n>['t']): string {
  if (key === 'name') return t('routes.tokens.fieldName');
  return t('routes.tokens.fieldToken');
}

export function TokenList({
  rows,
  activeId,
  onShowDetail,
  onDelete,
  installedAgents,
  onImport,
  onCreateForEndpoint,
  createPoolIdByKind,
  needRoute,
}: {
  rows: readonly LocalTokenRow[];
  activeId?: string | null;
  onShowDetail?: (row: LocalTokenRow) => void;
  onDelete?: (row: LocalTokenRow) => void;
  installedAgents?: readonly TokenImportAgentRef[];
  onImport?: (row: LocalTokenRow, agentId: AgentKey, draft: ConnectApiKeyDraft) => void;
  onCreateForEndpoint?: (row: LocalTokenRow) => void;
  createPoolIdByKind?: Readonly<Partial<Record<LocalEndpointKind, string>>>;
  needRoute?: boolean;
}) {
  const { t } = useI18n();
  const { toast } = useToast();
  const { widths, onResizeStart, totalWidth } = useColumnWidths(
    WIDTH_SPECS,
    COLUMN_WIDTHS_STORAGE_KEY,
  );
  const groups = buildLocalTokenGroups(rows);

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
          {groups.map((group) => {
            const endpoint = tokenEndpointParts({
              path: group.path,
              endpoint: group.endpoint,
              kind: group.kind,
            });
            const brandAgentId = localEndpointBrandAgentId(group.kind);
            const typeLabel = tokenTypeLabel({ kind: group.kind }, t);
            return (
              <Fragment key={group.kind}>
                <TableRow data-token-group={group.kind}>
                  <TableCell colSpan={2} className="bg-subtle">
                    <div className="flex min-w-0 flex-wrap items-center justify-between gap-x-3 gap-y-1">
                      <p className="shrink-0 font-medium text-primary">{typeLabel}</p>
                      <div className="min-w-0 max-w-full">
                        {endpoint.portPending ? (
                          <RouteEndpointUrl
                            path={group.path}
                            port={null}
                            host={endpoint.host}
                            endpointId={endpoint.endpointId}
                            brandAgentId={brandAgentId}
                            className="text-meta"
                          />
                        ) : (
                          <CopyableRouteEndpointUrl
                            path={group.path}
                            port={Number(endpoint.portLabel)}
                            host={endpoint.host}
                            endpointId={endpoint.endpointId}
                            brandAgentId={brandAgentId}
                            className="text-meta"
                          />
                        )}
                      </div>
                    </div>
                  </TableCell>
                </TableRow>
                {group.rows.map((row) => (
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
                        className="min-w-0"
                      >
                        {renderColumn(spec.key, row, {
                          t,
                          toast,
                          rows,
                          installedAgents,
                          onDelete,
                          onImport,
                          onCreateForEndpoint,
                          createPoolIdByKind,
                          needRoute,
                        })}
                      </TableCell>
                    ))}
                  </TableRow>
                ))}
              </Fragment>
            );
          })}
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
    rows: readonly LocalTokenRow[];
    installedAgents?: readonly TokenImportAgentRef[];
    onDelete?: (row: LocalTokenRow) => void;
    onImport?: (row: LocalTokenRow, agentId: AgentKey, draft: ConnectApiKeyDraft) => void;
    onCreateForEndpoint?: (row: LocalTokenRow) => void;
    createPoolIdByKind?: Readonly<Partial<Record<LocalEndpointKind, string>>>;
    needRoute?: boolean;
  },
): ReactNode {
  const { t } = ctx;
  if (key === 'name') {
    const label = tokenDisplayName(row, t);
    const showDefaultMark = row.primary && row.name.trim() && row.name.trim() !== t('routes.tokens.defaultName');
    return (
      <div className="flex min-w-0 items-baseline gap-1.5">
        <Tip className="min-w-0 truncate font-medium text-primary" label={label}>
          {label}
        </Tip>
        {showDefaultMark ? (
          <span className="shrink-0 text-meta text-muted">{t('routes.tokens.defaultName')}</span>
        ) : null}
      </div>
    );
  }
  if (row.unavailable && !row.maskedToken) {
    return <span className="text-meta text-muted">{t('routes.runtime.unavailable')}</span>;
  }
  if (!row.token?.trim()) {
    const createGate = localTokenEmptyCreateGate(
      row,
      ctx.createPoolIdByKind ?? {},
      t,
      ctx.needRoute,
    );
    return (
      <div className="flex min-w-0 flex-wrap items-center gap-2">
        <span className="text-meta text-muted">{t('routes.tokens.emptyTitle')}</span>
        {ctx.onCreateForEndpoint ? (
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="h-7 shrink-0"
            disabled={!createGate.enabled}
            title={createGate.reason ?? undefined}
            onClick={(event) => {
              event.stopPropagation();
              if (!createGate.enabled) {
                ctx.toast({
                  title: createGate.reason ?? t('routes.tokens.createNeedPool'),
                  variant: 'danger',
                });
                return;
              }
              ctx.onCreateForEndpoint?.(row);
            }}
          >
            {t('routes.tokens.createForEndpoint')}
          </Button>
        ) : null}
      </div>
    );
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
  const deleteGate = localTokenDeleteGate(row, ctx.rows, t);
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
      {ctx.installedAgents && ctx.onImport ? (
        <TokenImportToAgentButton
          row={row}
          installedAgents={ctx.installedAgents}
          className="shrink-0"
          onImport={(agentId, draft) => ctx.onImport?.(row, agentId, draft)}
        />
      ) : null}
      {ctx.onDelete && deleteGate.enabled ? (
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
