import { useMemo } from 'react';
import { Tip } from '@/components/ui/tooltip';
import { useI18n } from '@/components/shared/LanguageProvider';
import { RouteTraceFlowDiagram } from '@/components/shared/RouteTraceFlowDiagram';
import { formatInboundAt } from '@/pages/routes/shared/route-endpoint-copy';
import type {
  AdapterBridgeRouteTrace,
  RouteTraceStageStatus,
} from '@/lib/backend/contracts/adapter';
import { cn } from '@/lib/utils';

export type RouteTraceListItem = AdapterBridgeRouteTrace & {
  sourceLabel?: string;
  legacySummary?: boolean;
  unauthenticated?: boolean;
};

function failureStageLabel(
  stage: string | null | undefined,
  t: ReturnType<typeof useI18n>['t'],
): string | null {
  if (!stage) return null;
  switch (stage) {
    case 'local_endpoint':
      return t('routes.trace.stageId.local_endpoint');
    case 'local_auth':
      return t('routes.trace.stageId.local_auth');
    case 'pool':
      return t('routes.trace.stageId.pool');
    case 'request_conversion':
      return t('routes.trace.detailStage.requestConversion');
    case 'upstream_request':
      return t('routes.trace.detailStage.upstreamRequest');
    case 'upstream_response':
      return t('routes.trace.detailStage.upstreamResponse');
    case 'admission':
      return t('routes.trace.detailStage.admission');
    case 'route_resolution':
      return t('routes.trace.detailStage.routeResolution');
    case 'response_conversion':
      return t('routes.trace.detailStage.responseConversion');
    case 'delivery':
      return t('routes.trace.detailStage.delivery');
    default:
      return stage;
  }
}

function stageTone(status: RouteTraceStageStatus): string {
  switch (status) {
    case 'ok':
      return 'text-success';
    case 'failed':
      return 'text-danger';
    case 'skipped':
      return 'text-muted';
    default:
      return 'text-secondary';
  }
}

function TraceTextDetail({ row }: { row: RouteTraceListItem }) {
  const { t } = useI18n();
  const lines = useMemo(() => {
    const out: string[] = [];
    if (row.pool.attempts?.length) {
      for (const [index, attempt] of row.pool.attempts.entries()) {
        out.push(
          `${t('routes.trace.attempt', { n: index + 1 })}: ${attempt.member.label} · ${attempt.status}${attempt.code ? ` (${attempt.code})` : ''}`,
        );
      }
    }
    if (row.conversion.message) out.push(row.conversion.message);
    if (row.upstream.message) out.push(row.upstream.message);
    return out;
  }, [row, t]);
  if (lines.length === 0) return null;
  return (
    <ul className="mt-2 space-y-0.5 border-t border-border pt-2 text-meta text-muted">
      {lines.map((line) => (
        <li key={line} className="truncate">{line}</li>
      ))}
    </ul>
  );
}

/**
 * Route request traces for monitoring. Never shows Authorization, bodies, or keys.
 */
export function RouteTraceList({
  rows,
  emptyLabel,
  className,
}: {
  rows: readonly RouteTraceListItem[];
  emptyLabel?: string;
  className?: string;
}) {
  const { t } = useI18n();
  if (rows.length === 0) {
    return emptyLabel ? <p className="text-sm text-muted">{emptyLabel}</p> : null;
  }
  return (
    <ul className={cn('space-y-3', className)}>
      {rows.map((row) => (
        <li
          key={row.requestId}
          className="rounded-card border border-border bg-subtle p-3"
        >
          <div className="flex min-w-0 flex-wrap items-baseline gap-x-2 gap-y-0.5 font-mono text-meta">
            <span className="text-muted">{formatInboundAt(row.at)}</span>
            <span>{row.method}</span>
            <Tip label={row.path}>
              <span className="min-w-0 truncate">{row.path}</span>
            </Tip>
            <span className={stageTone(row.ok ? 'ok' : 'failed')}>{row.httpStatus}</span>
            <span className={row.ok ? 'text-success' : 'text-danger'}>
              {row.ok ? t('routes.inbound.ok') : t('routes.inbound.fail')}
            </span>
            {row.failureStage ? (
              <span className="text-danger">
                {t('routes.trace.failedAt', {
                  stage: failureStageLabel(row.failureStage, t) ?? row.failureStage,
                })}
              </span>
            ) : null}
            {row.model ? <span className="truncate text-muted">{row.model}</span> : null}
            {row.latencyMs != null ? (
              <span className="text-muted">{t('routes.trace.latencyMs', { ms: row.latencyMs })}</span>
            ) : null}
            {row.sourceLabel ? (
              <span className="truncate text-muted">{row.sourceLabel}</span>
            ) : null}
          </div>
          <RouteTraceFlowDiagram row={row} className="mt-3" />
          {!row.legacySummary && (row.pool.attempts?.length || row.conversion.message || row.upstream.message) ? (
            <details className="mt-2">
              <summary className="cursor-pointer text-meta text-secondary">
                {t('routes.trace.flow.flowMore')}
              </summary>
              <TraceTextDetail row={row} />
            </details>
          ) : null}
        </li>
      ))}
    </ul>
  );
}
