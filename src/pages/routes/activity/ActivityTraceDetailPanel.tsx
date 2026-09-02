import type { ReactNode } from 'react';
import { SideInspectPanel } from '@/components/layout/SideInspectPanel';
import {
  AbsoluteRouteEndpointUrl,
  CopyableRouteEndpointUrl,
} from '@/components/shared/RouteEndpointUrl';
import { useI18n } from '@/components/shared/LanguageProvider';
import type { RouteTraceListItem } from '@/components/shared/RouteTraceList';
import { routeEndpointHttpParts } from '@/lib/route-endpoints';
import { formatInboundAt } from '@/pages/bridges/route-endpoint-copy';
import {
  ACTIVITY_TRACE_STAGES,
  activityTraceConversionLabel,
  activityTraceLocalBrand,
  activityTraceModelLabel,
  activityTraceStageLabel,
  activityTraceStageStatusLabel,
  activityTraceUpstreamBrand,
  formatTraceSeconds,
  formatTraceTokens,
} from './activity-trace-list-model';
import { StageIcon, stageStatusOf, stageTone } from './ActivityTraceList';

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
  const inbound = routeEndpointHttpParts({
    path: row.path,
    port: row.localAuth.port,
  });
  const outbound = row.upstream.url?.trim() || '';
  const localBrand = activityTraceLocalBrand(row);
  const upstreamBrand = activityTraceUpstreamBrand(row);
  const conversion = activityTraceConversionLabel(row, t);
  const model = activityTraceModelLabel(row);
  const firstToken = formatTraceSeconds(row.ttftMs, t);
  const duration = formatTraceSeconds(row.latencyMs, t);
  const tokens = formatTraceTokens(row.inputTokens, row.outputTokens, t);
  const extra: string[] = [];
  if (row.pool.attempts?.length) {
    for (const [index, attempt] of row.pool.attempts.entries()) {
      extra.push(
        `${t('routes.trace.attempt', { n: index + 1 })}: ${attempt.member.label} · ${attempt.status}${attempt.code ? ` (${attempt.code})` : ''}`,
      );
    }
  }
  if (row.conversion.message) extra.push(row.conversion.message);
  if (row.upstream.message) extra.push(row.upstream.message);

  return (
    <SideInspectPanel
      title={t('routes.activity.detailTitle')}
      description={`${row.method} ${row.path}`}
      onClose={onClose}
      width={width}
    >
      <div className="flex flex-col gap-4" data-activity-trace-detail={row.requestId}>
        <dl className="flex flex-col gap-2">
          <Field label={t('routes.activity.colTime')}>
            <span className="font-mono">{formatInboundAt(row.at)}</span>
          </Field>
          <Field label={t('routes.activity.inboundEndpoint')}>
            <CopyableRouteEndpointUrl
              path={inbound.path}
              port={row.localAuth.port}
              host={inbound.host}
              endpointId={inbound.endpointId}
              brandAgentId={localBrand}
            />
          </Field>
          <Field label={t('routes.activity.outboundEndpoint')}>
            {outbound ? (
              <AbsoluteRouteEndpointUrl url={outbound} brandAgentId={upstreamBrand} />
            ) : (
              <span className="text-muted">—</span>
            )}
          </Field>
          <Field label={t('routes.activity.conversion')}>
            {conversion || <span className="text-muted">—</span>}
          </Field>
          <Field label={t('routes.activity.colModel')}>
            {model || <span className="text-muted">—</span>}
          </Field>
          <Field label={t('routes.activity.colFirstToken')}>
            {firstToken || <span className="text-muted">—</span>}
          </Field>
          <Field label={t('routes.activity.colDuration')}>
            {duration || <span className="text-muted">—</span>}
          </Field>
          <Field label={t('routes.activity.colTokens')}>
            {tokens || <span className="text-muted">—</span>}
          </Field>
        </dl>

        <section className="space-y-2">
          <h3 className="text-sm font-medium">{t('routes.activity.colStages')}</h3>
          <ul className="space-y-1.5" aria-label={t('routes.trace.pipelineAria')}>
            {ACTIVITY_TRACE_STAGES.map((stage) => {
              const status = stageStatusOf(row, stage);
              return (
                <li
                  key={stage}
                  className="flex items-center gap-2 rounded-btn border border-border px-2 py-1.5"
                  data-stage={stage}
                  data-stage-status={status}
                >
                  <StageIcon status={status} />
                  <span className="min-w-0 flex-1 truncate text-meta text-primary">
                    {activityTraceStageLabel(stage, t)}
                  </span>
                  <span className={`shrink-0 text-meta ${stageTone(status)}`}>
                    {activityTraceStageStatusLabel(status, t)}
                  </span>
                </li>
              );
            })}
          </ul>
        </section>

        {extra.length > 0 ? (
          <ul className="space-y-0.5 border-t border-border pt-2 text-meta text-muted">
            {extra.map((line) => (
              <li key={line} className="break-all">{line}</li>
            ))}
          </ul>
        ) : null}
      </div>
    </SideInspectPanel>
  );
}
