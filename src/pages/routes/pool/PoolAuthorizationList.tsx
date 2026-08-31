import type { KeyboardEvent, MouseEvent, ReactNode } from 'react';
import { agentDisplayName } from '@/config/agents';
import { AgentDot } from '@/components/shared/AgentDot';
import { StatusPin } from '@/components/shared/StatusPin';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Switch } from '@/components/ui/switch';
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
import { connectionKindLabel } from '@/lib/connection-kind';
import { adapterStatusTextClass } from '@/pages/bridges/adapter-view-model';
import {
  poolAuthorizationStatusView,
  type PoolAuthorizationItem,
} from '@/pages/bridges/route-pool-view-model';
import {
  formatPoolTimestamp,
  poolAuthorizationColumnLabel,
  poolAuthorizationQuotaParts,
  poolAuthorizationVisibleColumns,
  type PoolAuthorizationColumnKey,
} from './pool-authorization-detail';

const WIDTH_SPECS: ColumnWidthSpec<PoolAuthorizationColumnKey>[] = [
  { key: 'login', defaultWidth: 220, minWidth: 140 },
  { key: 'kind', defaultWidth: 104, minWidth: 80 },
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

const COLUMN_WIDTHS_STORAGE_KEY = 'agenthub.routes.pool.columnWidths';

function isInteractiveTableTarget(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) return false;
  return Boolean(
    target.closest('button, a, input, textarea, [role="button"], [role="switch"], [role="menuitem"]'),
  );
}

function EmptyCell() {
  return <span className="text-muted">—</span>;
}

function cellValue(value: ReactNode): ReactNode {
  if (value == null || value === '') return <EmptyCell />;
  return value;
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

  const openItem = (item: PoolAuthorizationItem) => {
    onShowDetail?.(item);
  };

  return (
    <TableShell className="min-w-0 [&>div]:min-w-0 [&>div]:!overflow-x-scroll">
      <Table
        className="table-fixed"
        style={{ width: `max(100%, ${totalWidth}px)`, minWidth: totalWidth }}
      >
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
          {items.map((item) => {
            const status = poolAuthorizationStatusView(item, t);
            return (
              <TableRow
                key={item.key}
                data-pool-authorization={item.key}
                active={activeKey === item.key}
                tabIndex={onShowDetail ? 0 : undefined}
                className={onShowDetail ? 'cursor-pointer' : undefined}
                onClick={onShowDetail ? (event: MouseEvent<HTMLTableRowElement>) => {
                  if (event.defaultPrevented) return;
                  if (isInteractiveTableTarget(event.target)) return;
                  openItem(item);
                } : undefined}
                onKeyDown={onShowDetail ? (event: KeyboardEvent<HTMLTableRowElement>) => {
                  if (event.key !== 'Enter' && event.key !== ' ') return;
                  if (isInteractiveTableTarget(event.target)) return;
                  event.preventDefault();
                  openItem(item);
                } : undefined}
              >
                {columns.map((key) => (
                  <TableCell
                    key={key}
                    data-col={key}
                    className={key === 'login' ? 'min-w-0' : 'whitespace-nowrap'}
                    onClick={key === 'enabled' ? (event) => event.stopPropagation() : undefined}
                    onPointerDown={key === 'enabled' ? (event) => event.stopPropagation() : undefined}
                  >
                    {renderColumn(key, item, {
                      status,
                      toggling: togglingKey === item.key,
                      t,
                      onEnabledChange,
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
        <EmptyCell />
      );
    case 'login':
      return (
        <div className="flex min-w-0 items-center gap-2">
          <AgentDot agentId={item.agentId} size="sm" title={null} />
          <div className="min-w-0">
            <p className="truncate font-medium" title={item.identityLabel ?? item.title}>
              {item.identityLabel ?? item.title}
            </p>
            <p className="truncate text-meta text-muted">{agentDisplayName(item.agentId)}</p>
          </div>
        </div>
      );
    case 'kind':
      return <span className="text-meta text-secondary">{connectionKindLabel(item.kind, ctx.t)}</span>;
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
      if (parts.length === 0) return <EmptyCell />;
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
