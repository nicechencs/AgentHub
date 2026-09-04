import { useEffect, useMemo, useState, type ReactNode } from 'react';
import { SideInspectPanel } from '@/components/layout/SideInspectPanel';
import { useI18n } from '@/components/shared/LanguageProvider';
import type { RouteTraceListItem } from '@/components/shared/RouteTraceList';
import type { RouteTraceStageId } from '@/lib/backend/contracts/adapter';
import { formatInboundAt } from '@/pages/routes/shared/route-endpoint-copy';
import {
  activityTraceModelLabel,
  formatTraceSeconds,
  formatTraceTokens,
} from './activity-trace-list-model';
import {
  activityTraceFailureHeadline,
  summarizeActivityTrace,
} from './activity-trace-summary-model';
import { buildTraceStages } from './trace-detail/build-trace-stages';
import { TraceStageCard } from './trace-detail/TraceStageCard';

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="grid grid-cols-[6.5rem_minmax(0,1fr)] gap-x-3 gap-y-1 text-meta">
      <dt className="text-muted">{label}</dt>
      <dd className="min-w-0 break-all text-secondary">{children}</dd>
    </div>
  );
}

export function ActivityTraceDetailPanel({
  row,
  width,
  onClose,
}: {
  row: RouteTraceListItem;
  width: number;
  onClose: () => void;
}) {
  const { t } = useI18n();
  const model = activityTraceModelLabel(row);
  const duration = formatTraceSeconds(row.latencyMs, t);
  const tokens = formatTraceTokens(row.inputTokens, row.outputTokens, t);
  const summary = summarizeActivityTrace(row);
  const failureHeadline = activityTraceFailureHeadline(summary, t);
  const failureDefault = row.failureStage ?? null;
  const [expanded, setExpanded] = useState<Set<RouteTraceStageId>>(
    () => new Set<RouteTraceStageId>(failureDefault ? ['received', failureDefault] : ['received']),
  );

  useEffect(() => {
    setExpanded(new Set<RouteTraceStageId>(failureDefault ? ['received', failureDefault] : ['received']));
  }, [failureDefault, row.requestId]);

  const stages = useMemo(() => buildTraceStages(row, t), [row, t]);
  const toggle = (id: RouteTraceStageId) => {
    setExpanded((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  return (
    <SideInspectPanel
      title={t('routes.activity.detailTitle')}
      description={`${row.method} ${row.path}`}
      headerActions={summary.result !== 'success' ? (
        <span className="rounded-full border border-danger/30 bg-danger/5 px-1.5 py-0.5 text-meta font-medium text-danger">{t('routes.inbound.fail')}</span>
      ) : undefined}
      onClose={onClose}
      width={width}
    >
      <div className="flex flex-col gap-4" data-activity-trace-detail={row.requestId}>
        <dl className="flex flex-col gap-2">
          <Field label={t('routes.activity.colTime')}><span className="font-mono">{formatInboundAt(row.at)}</span></Field>
          <Field label={t('routes.activity.colModel')}>{model || <span className="text-muted">—</span>}</Field>
          <Field label={t('routes.activity.colDuration')}>{duration || <span className="text-muted">—</span>}</Field>
          <Field label={t('routes.activity.colTokens')}>{tokens || <span className="text-muted">—</span>}</Field>
        </dl>

        {failureHeadline ? (
          <section className="rounded-card border border-danger/20 border-l-2 border-l-danger bg-danger/5 px-2.5 py-2 text-meta">
            <p className="font-medium text-danger">{failureHeadline}</p>
            {summary.errorMessage ? <p className="mt-1 break-all text-secondary">{summary.errorMessage}</p> : null}
          </section>
        ) : null}

        <section className="space-y-2">
          <div>
            <h3 className="text-sm font-medium">{t('routes.activity.requestPath')}</h3>
            <p className="mt-0.5 text-meta text-muted">{t('routes.trace.nodeDetailHint')}</p>
          </div>
          <ol className="space-y-2" aria-label={t('routes.trace.detailPipelineAria')}>
            {stages.map((stage, index) => (
              <TraceStageCard
                key={stage.id}
                stage={stage}
                index={index}
                expanded={expanded.has(stage.id)}
                onToggle={() => toggle(stage.id)}
              />
            ))}
          </ol>
        </section>
      </div>
    </SideInspectPanel>
  );
}
