import { useCallback, useEffect, useMemo, type ReactNode } from 'react';
import { CircleUser, KeyRound } from 'lucide-react';
import { SortHandle } from '@/components/shared/SortHandle';
import { SORTABLE_ID_ATTR, useSortableDrag } from '@/components/shared/use-sortable-drag';
import { useStoredIdOrder } from '@/components/shared/use-stored-id-order';
import { StatusPin } from '@/components/shared/StatusPin';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Switch } from '@/components/ui/switch';
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
import { resolveAgentMeta } from '@/config/agents';
import { applyIdOrder } from '@/lib/list-order';
import { connectionKindLabel, type ConnectionKind } from '@/lib/connection-kind';
import type { AgentKey } from '@/lib/types';
import { adapterStatusTextClass } from '@/pages/routes/shared/adapter-view-model';
import {
  poolAuthorizationStatusView,
  type PoolAuthorizationItem,
} from '@/pages/routes/shared/route-pool-view-model';
import { PoolEndpointTypeLine } from './PoolEndpointTypeLine';
import { PoolLoginMark } from './PoolLoginMark';
import {
  formatPoolTimestamp,
  poolAuthorizationColumnLabel,
  poolAuthorizationEndpointKinds,
  poolAuthorizationLoginLabel,
  poolAuthorizationQuotaParts,
  poolAuthorizationVisibleColumns,
  type PoolAuthorizationColumnKey,
} from './pool-authorization-detail';
import { StorageKey } from '@/lib/ui-preferences';

const WIDTH_SPECS: ColumnWidthSpec<PoolAuthorizationColumnKey>[] = [
  { key: 'login', defaultWidth: 240, minWidth: 160 },
  { key: 'kind', defaultWidth: 128, minWidth: 96 },
  { key: 'endpointTypes', defaultWidth: 200, minWidth: 140 },
  { key: 'status', defaultWidth: 104, minWidth: 80 },
  { key: 'bindings', defaultWidth: 96, minWidth: 72 },
  { key: 'quota', defaultWidth: 128, minWidth: 96 },
  { key: 'lastUsed', defaultWidth: 148, minWidth: 120 },
  { key: 'priority', defaultWidth: 80, minWidth: 64 },
  { key: 'enabled', defaultWidth: 64, minWidth: 52 },
];

const WIDTH_BY_KEY = Object.fromEntries(
  WIDTH_SPECS.map((spec) => [spec.key, spec]),
) as Record<PoolAuthorizationColumnKey, ColumnWidthSpec<PoolAuthorizationColumnKey>>;

const COLUMN_WIDTHS_STORAGE_KEY = StorageKey.routesPoolColumnWidths;
const ORDER_STORAGE_KEY = StorageKey.routesPoolAuthorizationOrder;

function cellValue(value: ReactNode): ReactNode {
  if (value == null || value === '') return <TableEmptyCell />;
  return value;
}

function KindMark({
  kind,
  agentId,
}: {
  kind: ConnectionKind;
  agentId: AgentKey;
}) {
  const { t } = useI18n();
  const color = resolveAgentMeta(agentId).color;
  const oauth = kind === 'oauth';
  const label = oauth ? t('kind.oauth') : t('kind.apikey');
  const Icon = oauth ? CircleUser : KeyRound;
  return (
    <Hint label={label}>
      <span className="inline-flex" style={{ color }} aria-label={label} data-pool-kind-mark={kind}>
        <Icon className="h-4 w-4" strokeWidth={1.8} />
      </span>
    </Hint>
  );
}

export function PoolAuthorizationList({
  items,
  activeKey,
  togglingKey,
  onShowDetail,
  onEnabledChange,
}: {
  items: readonly PoolAuthorizationItem[];
  activeKey?: string | null;
  togglingKey?: string | null;
  onShowDetail?: (item: PoolAuthorizationItem) => void;
  onEnabledChange?: (item: PoolAuthorizationItem, enabled: boolean) => void;
}) {
  const { t } = useI18n();
  const columns = poolAuthorizationVisibleColumns(items);
  const specs = columns.map((key) => WIDTH_BY_KEY[key]);
  const { widths, onResizeStart } = useColumnWidths(
    WIDTH_SPECS,
    COLUMN_WIDTHS_STORAGE_KEY,
  );
  const totalWidth = specs.reduce((sum, spec) => sum + widths[spec.key], 0);
  const { stored, moveInLive, seedIfEmpty } = useStoredIdOrder(ORDER_STORAGE_KEY);
  const rows = useMemo(
    () => applyIdOrder([...items], (item) => item.key, stored),
    [items, stored],
  );
  const liveIds = useMemo(() => rows.map((item) => item.key), [rows]);
  useEffect(() => {
    seedIfEmpty(liveIds);
  }, [liveIds, seedIfEmpty]);
  const canReorder = liveIds.length > 1;
  const { onDragStartId, rowProps } = useSortableDrag((fromId, toId) => {
    moveInLive(liveIds, fromId, toId);
  });
  const moveNeighbor = useCallback((id: string, direction: -1 | 1) => {
    const index = liveIds.indexOf(id);
    const next = liveIds[index + direction];
    if (!next) return;
    moveInLive(liveIds, id, next);
  }, [liveIds, moveInLive]);

  return (
    <TableShell layout="split">
      <Table className="table-fixed" style={{ minWidth: totalWidth }}>
        <colgroup>
          {specs.map((spec) => (
            <col key={spec.key} style={{ width: widths[spec.key] }} />
          ))}
        </colgroup>
        <TableHeader>
          <TableHeaderRow>
            {columns.map((key) => {
              const label = poolAuthorizationColumnLabel(key, t);
              return (
                <TableHead key={key} className="relative select-none" data-col={key}>
                  {label}
                  <ColumnResizeHandle
                    columnKey={key}
                    label={label}
                    onResizeStart={onResizeStart}
                  />
                </TableHead>
              );
            })}
          </TableHeaderRow>
        </TableHeader>
        <TableBody>
          {rows.map((item) => {
            const status = poolAuthorizationStatusView(item, t);
            const sortable = rowProps(item.key);
            return (
              <TableRow
                key={item.key}
                data-pool-authorization={item.key}
                active={activeKey === item.key}
                className={sortable.className}
                {...{ [SORTABLE_ID_ATTR]: sortable[SORTABLE_ID_ATTR] }}
              >
                {columns.map((key) => (
                  <TableCell
                    key={key}
                    data-col={key}
                    className={key === 'login' || key === 'endpointTypes' ? 'min-w-0' : 'whitespace-nowrap'}
                    onClick={key === 'enabled' ? (event) => event.stopPropagation() : undefined}
                    onPointerDown={key === 'enabled' ? (event) => event.stopPropagation() : undefined}
                  >
                    {renderColumn(key, item, {
                      status,
                      toggling: togglingKey === item.key,
                      t,
                      onEnabledChange,
                      onShowDetail,
                      sortHandle: canReorder && key === 'login' ? (
                        <SortHandle
                          id={item.key}
                          onDragStartId={onDragStartId}
                          onMoveNeighbor={moveNeighbor}
                        />
                      ) : null,
                    })}
                  </TableCell>
                ))}
              </TableRow>
            );
          })}
        </TableBody>
      </Table>
    </TableShell>
  );
}

function renderColumn(
  key: PoolAuthorizationColumnKey,
  item: PoolAuthorizationItem,
  ctx: {
    status: ReturnType<typeof poolAuthorizationStatusView>;
    toggling: boolean;
    t: ReturnType<typeof useI18n>['t'];
    onEnabledChange?: (item: PoolAuthorizationItem, enabled: boolean) => void;
    onShowDetail?: (item: PoolAuthorizationItem) => void;
    sortHandle?: ReactNode;
  },
): ReactNode {
  switch (key) {
    case 'enabled':
      return item.canToggle ? (
        <Switch
          checked={item.enabled !== false}
          disabled={ctx.toggling}
          onCheckedChange={(enabled) => ctx.onEnabledChange?.(item, enabled)}
          aria-label={ctx.t('routes.pool.detail.enabled')}
        />
      ) : (
        <TableEmptyCell />
      );
    case 'login': {
      const loginLabel = poolAuthorizationLoginLabel(item);
      return (
        <div className="flex min-w-0 items-center gap-2">
          {ctx.sortHandle}
          <PoolLoginMark item={item} />
          {ctx.onShowDetail ? (
            <Tip className="min-w-0" label={loginLabel}>
              <button
                type="button"
                data-pool-login-name={item.key}
                className="max-w-full truncate text-left font-medium text-primary hover:underline"
                onClick={() => ctx.onShowDetail?.(item)}
              >
                {loginLabel}
              </button>
            </Tip>
          ) : (
            <Tip className="truncate font-medium" label={loginLabel}>
              {loginLabel}
            </Tip>
          )}
        </div>
      );
    }
    case 'kind':
      return (
        <div className="flex items-center gap-1.5">
          <KindMark kind={item.kind} agentId={item.agentId} />
          <span className="text-meta text-secondary">{connectionKindLabel(item.kind, ctx.t)}</span>
        </div>
      );
    case 'endpointTypes': {
      const kinds = poolAuthorizationEndpointKinds(item);
      if (kinds.length === 0) return <TableEmptyCell />;
      return (
        <div className="flex flex-col gap-0.5 text-meta">
          {kinds.map((kind) => (
            <PoolEndpointTypeLine key={kind} kind={kind} />
          ))}
        </div>
      );
    }
    case 'status':
      return (
        <div className="flex items-center gap-1.5">
          <StatusPin tone={ctx.status.tone} size="md" />
          <span className={adapterStatusTextClass(ctx.status.tone)}>{ctx.status.label}</span>
        </div>
      );
    case 'bindings':
      return cellValue(
        item.bindingCount && item.bindingCount > 0 ? String(item.bindingCount) : null,
      );
    case 'quota': {
      const parts = poolAuthorizationQuotaParts(item);
      if (parts.length === 0) return <TableEmptyCell />;
      return (
        <div className="flex flex-col gap-0.5 text-meta text-secondary">
          {parts.map((part) => (
            <span key={part}>{part}</span>
          ))}
        </div>
      );
    }
    case 'lastUsed':
      return (
        <span className="text-meta text-secondary">
          {cellValue(formatPoolTimestamp(item.lastUsedAt))}
        </span>
      );
    case 'priority':
      return cellValue(item.priority == null ? null : String(item.priority));
  }
}
