import { Tip } from '@/components/ui/tooltip';
import { useI18n } from '@/components/shared/LanguageProvider';
import { formatInboundAt } from '@/pages/bridges/route-endpoint-copy';
import type { AdapterBridgeInboundRequest } from '@/lib/backend/contracts/adapter';
import { cn } from '@/lib/utils';

export type InboundRequestListItem = AdapterBridgeInboundRequest & {
  /** Optional route label shown at the end of the row. */
  sourceLabel?: string;
};

/**
 * Shared inbound request rows for route detail, board, and activity.
 * Never shows Authorization, bodies, or keys.
 */
export function InboundRequestList({
  rows,
  emptyLabel,
  className,
}: {
  rows: readonly InboundRequestListItem[];
  emptyLabel?: string;
  className?: string;
}) {
  const { t } = useI18n();
  if (rows.length === 0) {
    return emptyLabel ? <p className="text-sm text-muted">{emptyLabel}</p> : null;
  }
  return (
    <ul className={cn('space-y-1 rounded-card border border-border bg-subtle p-3', className)}>
      {rows.map((row, index) => (
        <li
          key={`${row.at}:${row.method}:${row.path}:${row.status}:${row.sourceLabel ?? ''}:${index}`}
          className="flex min-w-0 flex-wrap items-baseline gap-x-2 gap-y-0.5 font-mono text-meta"
        >
          <span className="text-muted">{formatInboundAt(row.at)}</span>
          <span>{row.method}</span>
          <Tip label={row.path}>
            <span className="min-w-0 truncate">{row.path}</span>
          </Tip>
          <span>{row.status}</span>
          <span className={row.ok ? 'text-success' : 'text-danger'}>
            {row.ok ? t('routes.inbound.ok') : t('routes.inbound.fail')}
          </span>
          {row.sourceLabel ? (
            <span className="truncate text-muted">{row.sourceLabel}</span>
          ) : null}
        </li>
      ))}
    </ul>
  );
}
