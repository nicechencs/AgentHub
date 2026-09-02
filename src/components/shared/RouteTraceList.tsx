import { useMemo } from 'react';
import { Tip } from '@/components/ui/tooltip';
import { useI18n } from '@/components/shared/LanguageProvider';
import { formatInboundAt } from '@/pages/bridges/route-endpoint-copy';
import type {
  AdapterBridgeRouteTrace,
  RouteTraceStageStatus,
} from '@/lib/backend/contracts/adapter';
import { cn } from '@/lib/utils';

export type RouteTraceListItem = AdapterBridgeRouteTrace & {
  sourceLabel?: string;
  legacySummary?: boolean;
};

function stageTone(status: RouteTraceStageStatus): string {
  switch (status) {
    case 'ok':
      return 'border-success/40 bg-success/10 text-success';
    case 'failed':
      return 'border-danger/40 bg-danger/10 text-danger';
    case 'skipped':
      return 'border-border bg-subtle text-muted';
    default:
      return 'border-border bg-panel text-secondary';
  }
}

function StageBadge({
  label,
  status,
}: {
  label: string;
  status: RouteTraceStageStatus;
}) {
  return (
    <span className={cn('rounded-full border px-2 py-0.5 text-meta font-medium', stageTone(status))}>
      {label}
    </span>
  );
}

function StageStrip({ row }: { row: RouteTraceListItem }) {
  const { t } = useI18n();
  const stages = [
    { key: 'localAuth', label: t('routes.trace.stageLocalAuth'), status: row.localAuth.status },
    { key: 'pool', label: t('routes.trace.stagePool'), status: row.pool.status },
    { key: 'conversion', label: t('routes.trace.stageConversion'), status: row.conversion.status },
    { key: 'upstreamAuth', label: t('routes.trace.stageUpstreamAuth'), status: row.upstreamAuth.status },
    { key: 'upstream', label: t('routes.trace.stageUpstream'), status: row.upstream.status },
  ];
  return (
    <div className="mt-1 flex w-full flex-wrap gap-1" aria-label={t('routes.trace.pipelineAria')}>
      {stages.map((stage) => (
        <StageBadge key={stage.key} label={stage.label} status={stage.status} />
      ))}
      {row.legacySummary ? (
        <span className="text-meta text-muted">{t('routes.trace.legacySummary')}</span>
      ) : null}
    </div>
  );
}

function failureStageLabel(
  stage: string | null | undefined,
  t: ReturnType<typeof useI18n>['t'],
): string | null {
  if (!stage) return null;
  switch (stage) {
    case 'local_auth':
      return t('routes.trace.stageId.local_auth');
    case 'pool':
      return t('routes.trace.stageId.pool');
    case 'conversion':
      return t('routes.trace.stageId.conversion');
    case 'upstream_auth':
      return t('routes.trace.stageId.upstream_auth');
    case 'upstream':
      return t('routes.trace.stageId.upstream');
    default:
      return stage;
  }
}

function TraceDetail({
  row,
}: {
  row: RouteTraceListItem;
}) {
  const { t } = useI18n();
  const stages = useMemo(
    () => [
      {
        id: 'localAuth',
        label: t('routes.trace.stageLocalAuth'),
        status: row.localAuth.status,
        lines: [
          row.localAuth.code ? `${t('routes.trace.result')}: ${row.localAuth.code}` : null,
          row.localAuth.message,
        ].filter(Boolean) as string[],
      },
      {
        id: 'pool',
        label: t('routes.trace.stagePool'),
        status: row.pool.status,
        lines: [
          row.pool.selectedMember?.label
            ? `${t('routes.trace.selected')}: ${row.pool.selectedMember.label}`
            : null,
          row.pool.code ? `${t('routes.trace.result')}: ${row.pool.code}` : null,
          row.pool.message,
          ...(row.pool.attempts ?? []).flatMap((attempt, index) => [
            `${t('routes.trace.attempt', { n: index + 1 })}: ${attempt.member.label} · ${attempt.status}${attempt.code ? ` (${attempt.code})` : ''}`,
          ]),
        ].filter(Boolean) as string[],
      },
      {
        id: 'conversion',
        label: t('routes.trace.stageConversion'),
        status: row.conversion.status,
        lines: [
          row.conversion.path ? `${t('routes.trace.path')}: ${row.conversion.path}` : null,
          row.conversion.result ? `${t('routes.trace.result')}: ${row.conversion.result}` : null,
          row.conversion.code,
          row.conversion.message,
        ].filter(Boolean) as string[],
      },
      {
        id: 'upstreamAuth',
        label: t('routes.trace.stageUpstreamAuth'),
        status: row.upstreamAuth.status,
        lines: [
          row.upstreamAuth.httpStatus != null
            ? `HTTP ${row.upstreamAuth.httpStatus}`
            : null,
          row.upstreamAuth.code ? `${t('routes.trace.result')}: ${row.upstreamAuth.code}` : null,
          row.upstreamAuth.message,
        ].filter(Boolean) as string[],
      },
      {
        id: 'upstream',
        label: t('routes.trace.stageUpstream'),
        status: row.upstream.status,
        lines: [
          row.upstream.url,
          row.upstream.member?.label
            ? `${t('routes.trace.account')}: ${row.upstream.member.label}`
            : null,
          row.upstream.upstreamModel
            ? `${t('routes.trace.upstreamModel')}: ${row.upstream.upstreamModel}`
            : null,
          row.upstream.httpStatus != null ? `HTTP ${row.upstream.httpStatus}` : null,
          row.upstream.code ? `${t('routes.trace.result')}: ${row.upstream.code}` : null,
          row.upstream.message,
        ].filter(Boolean) as string[],
      },
    ],
    [row, t],
  );

  return (
    <ol className="mt-2 space-y-2 border-t border-border pt-2">
      {stages.map((stage) => (
        <li key={stage.id} className="space-y-0.5">
          <div className="flex flex-wrap items-center gap-2">
            <StageBadge label={stage.label} status={stage.status} />
            {stage.lines[0] ? (
              <span className="min-w-0 truncate text-meta text-secondary">{stage.lines[0]}</span>
            ) : null}
          </div>
          {stage.lines.length > 1 ? (
            <ul className="ml-1 space-y-0.5 text-meta text-muted">
              {stage.lines.slice(1).map((line) => (
                <li key={line} className="truncate">
                  {line}
                </li>
              ))}
            </ul>
          ) : null}
        </li>
      ))}
    </ol>
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
    <ul className={cn('space-y-1 rounded-card border border-border bg-subtle p-3', className)}>
      {rows.map((row) => (
        <li key={row.requestId}>
          <details className="group">
            <summary className="block cursor-pointer list-none marker:content-none [&::-webkit-details-marker]:hidden">
              <div className="flex min-w-0 flex-wrap items-baseline gap-x-2 gap-y-0.5 font-mono text-meta">
              <span className="text-muted">{formatInboundAt(row.at)}</span>
              <span>{row.method}</span>
              <Tip label={row.path}>
                <span className="min-w-0 truncate">{row.path}</span>
              </Tip>
              <span>{row.httpStatus}</span>
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
              <StageStrip row={row} />
            </summary>
            {!row.legacySummary ? <TraceDetail row={row} /> : null}
          </details>
        </li>
      ))}
    </ul>
  );
}
