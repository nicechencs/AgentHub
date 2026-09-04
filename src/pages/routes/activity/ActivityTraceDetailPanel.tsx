import { ChevronDown, Minus } from 'lucide-react';
import { useEffect, useMemo, useState, type ReactNode } from 'react';
import { SideInspectPanel } from '@/components/layout/SideInspectPanel';
import { CopyableRouteEndpointUrl } from '@/components/shared/RouteEndpointUrl';
import { useI18n } from '@/components/shared/LanguageProvider';
import type { RouteTraceListItem } from '@/components/shared/RouteTraceList';
import type { RouteTraceStageStatus } from '@/lib/backend/contracts/adapter';
import { routeEndpointHttpParts } from '@/lib/route-endpoints';
import { cn } from '@/lib/utils';
import { formatInboundAt } from '@/pages/routes/shared/route-endpoint-copy';
import {
  activityTraceConversionLabel,
  activityTraceLocalBrand,
  activityTraceModelLabel,
  activityTraceStageStatusLabel,
  formatTraceSeconds,
  formatTraceTokens,
} from './activity-trace-list-model';
import { ActivityTraceStageIcon, activityTraceStageTone } from './ActivityTraceStageDisplay';
import {
  activityTraceFailureHeadline,
  activityTraceStageStatus,
  summarizeActivityTrace,
} from './activity-trace-summary-model';

type DetailStageId =
  | 'received'
  | 'local_auth'
  | 'local_endpoint'
  | 'admission'
  | 'route_resolution'
  | 'pool'
  | 'request_conversion'
  | 'upstream_request'
  | 'upstream_response'
  | 'response_conversion'
  | 'delivery';

type DetailStageStatus = RouteTraceStageStatus | 'unrecorded';

type DetailStage = {
  id: DetailStageId;
  title: string;
  status: DetailStageStatus;
  summary?: string | null;
  details: ReactNode;
};

function keyHint(last4?: string | null): string {
  return last4?.trim() ? `••••${last4.trim()}` : '—';
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="grid grid-cols-[6.5rem_minmax(0,1fr)] gap-x-3 gap-y-1 text-meta">
      <dt className="text-muted">{label}</dt>
      <dd className="min-w-0 break-all text-secondary">{children}</dd>
    </div>
  );
}

function statusCardTone(status: DetailStageStatus): string {
  if (status === 'ok') return 'border-success/40 bg-success/5';
  if (status === 'failed') return 'border-danger/50 bg-danger/5';
  if (status === 'skipped') return 'border-border/60 bg-subtle/40 opacity-70';
  if (status === 'unrecorded') return 'border-border/60 bg-subtle/30';
  return 'border-accent/40 bg-accent/5';
}

function detailStatusLabel(status: DetailStageStatus, t: ReturnType<typeof useI18n>['t']): string {
  return status === 'unrecorded'
    ? t('routes.trace.detail.unrecorded')
    : activityTraceStageStatusLabel(status, t);
}

function TraceStageCard({
  stage,
  index,
  expanded,
  onToggle,
}: {
  stage: DetailStage;
  index: number;
  expanded: boolean;
  onToggle: () => void;
}) {
  const { t } = useI18n();
  return (
    <li className="relative pl-5" data-detail-stage={stage.id} data-stage-status={stage.status}>
      {index > 0 ? <span className="absolute bottom-1/2 left-[0.4375rem] top-[-0.75rem] w-px bg-border" aria-hidden /> : null}
      <span className="absolute left-0 top-3.5 z-10 flex h-3.5 w-3.5 items-center justify-center rounded-full bg-panel" aria-hidden>
        {stage.status === 'unrecorded'
          ? <Minus className="h-3.5 w-3.5 text-muted" />
          : <ActivityTraceStageIcon status={stage.status} />}
      </span>
      <button
        type="button"
        className={cn(
          'w-full rounded-card border px-3 py-2.5 text-left transition-colors hover:border-accent/60 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent',
          statusCardTone(stage.status),
        )}
        aria-expanded={expanded}
        aria-controls={`trace-stage-${stage.id}`}
        onClick={onToggle}
      >
        <span className="flex items-center gap-2">
          <span className="min-w-0 flex-1">
            <span className="block text-sm font-medium text-primary">{stage.title}</span>
            {stage.summary ? <span className="mt-0.5 block truncate text-meta text-muted">{stage.summary}</span> : null}
          </span>
          <span className={cn('shrink-0 text-meta', stage.status === 'unrecorded' ? 'text-muted' : activityTraceStageTone(stage.status))}>
            {detailStatusLabel(stage.status, t)}
          </span>
          <ChevronDown className={cn('h-4 w-4 shrink-0 text-muted transition-transform', expanded && 'rotate-180')} aria-hidden />
        </span>
      </button>
      {expanded ? (
        <div id={`trace-stage-${stage.id}`} className="mx-1 rounded-b-card border border-t-0 border-border bg-panel px-3 py-2.5">
          <dl className="flex flex-col gap-1.5">{stage.details}</dl>
        </div>
      ) : null}
    </li>
  );
}

function responseResultLabel(value: string | null | undefined, t: ReturnType<typeof useI18n>['t']): string {
  if (value === 'completed') return t('routes.trace.detail.completed');
  if (value === 'streaming') return t('routes.trace.detail.streaming');
  if (value === 'failed') return t('routes.inbound.fail');
  if (value === 'interrupted') return t('routes.trace.detail.interrupted');
  return value || '—';
}

function completionLabel(value: string | null | undefined, t: ReturnType<typeof useI18n>['t']): string {
  if (value === 'response_returned') return t('routes.trace.detail.responseReturned');
  if (value === 'stream_completed') return t('routes.trace.detail.streamCompleted');
  if (value === 'stream_error') return t('routes.trace.detail.streamError');
  if (value === 'client_disconnected') return t('routes.trace.detail.clientDisconnected');
  if (value === 'streaming') return t('routes.trace.detail.streaming');
  return value || t('routes.trace.detail.noSeparateData');
}

function fallbackEndpointStatus(row: RouteTraceListItem): DetailStageStatus {
  if (row.legacySummary) return 'skipped';
  if (row.failureStage === 'local_endpoint') return 'failed';
  return 'unrecorded';
}

function fallbackLaterStatus(row: RouteTraceListItem): DetailStageStatus {
  return row.legacySummary ? 'skipped' : 'unrecorded';
}

function failureDetailStage(failureStage?: string | null): DetailStageId | null {
  if (failureStage === 'conversion') return 'request_conversion';
  if (failureStage === 'upstream_auth' || failureStage === 'upstream') return 'upstream_response';
  if (failureStage === 'response_conversion') return 'response_conversion';
  if (failureStage === 'local_auth' || failureStage === 'local_endpoint'
    || failureStage === 'admission' || failureStage === 'route_resolution'
    || failureStage === 'pool' || failureStage === 'delivery') return failureStage;
  return null;
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
  const inbound = routeEndpointHttpParts({ path: row.path, port: row.localAuth.port });
  const selectedLogin = row.upstream.member ?? row.pool.selectedMember;
  const localBrand = activityTraceLocalBrand(row);
  const model = activityTraceModelLabel(row);
  const firstToken = formatTraceSeconds(row.ttftMs, t);
  const duration = formatTraceSeconds(row.latencyMs, t);
  const tokens = formatTraceTokens(row.inputTokens, row.outputTokens, t);
  const summary = summarizeActivityTrace(row);
  const failureHeadline = activityTraceFailureHeadline(summary, t);
  const failureDefault = failureDetailStage(row.failureStage);
  const [expanded, setExpanded] = useState<Set<DetailStageId>>(
    () => new Set<DetailStageId>(failureDefault ? ['received', failureDefault] : ['received']),
  );
  useEffect(() => {
    setExpanded(new Set<DetailStageId>(failureDefault ? ['received', failureDefault] : ['received']));
  }, [failureDefault, row.requestId]);

  const stages = useMemo<DetailStage[]>(() => {
    const attempts = row.pool.attempts ?? [];
    const upstreamRequestStatus: RouteTraceStageStatus = row.upstream.status === 'skipped'
      ? 'skipped'
      : row.upstream.url || row.upstream.httpStatus != null
        ? 'ok'
        : row.upstream.status;
    return [
      {
        id: 'received',
        title: t('routes.trace.detailStage.received'),
        status: 'ok',
        summary: `${row.method} ${row.path}`,
        details: <>
          <Field label={t('routes.trace.detail.requestId')}><span className="font-mono">{row.requestId}</span></Field>
          <Field label={t('routes.activity.colTime')}><span className="font-mono">{formatInboundAt(row.at)}</span></Field>
          <Field label={t('routes.activity.colRequest')}>{row.method} {row.path}</Field>
        </>,
      },
      {
        id: 'local_auth',
        title: t('routes.trace.detailStage.localAuth'),
        status: activityTraceStageStatus(row, 'local_auth'),
        summary: keyHint(row.localAuth.keyLast4),
        details: <>
          <Field label={t('routes.activity.localKey')}><span className="font-mono">{keyHint(row.localAuth.keyLast4)}</span></Field>
          <Field label={t('routes.trace.detail.port')}>{row.localAuth.port ?? '—'}</Field>
          <Field label={t('routes.trace.detail.code')}>{row.localAuth.code || '—'}</Field>
          <Field label={t('routes.trace.detail.message')}>{row.localAuth.message || '—'}</Field>
        </>,
      },
      {
        id: 'local_endpoint',
        title: t('routes.trace.detailStage.localEndpoint'),
        status: row.localEndpoint?.status ?? fallbackEndpointStatus(row),
        summary: row.path,
        details: <>
          <Field label={t('routes.activity.inboundEndpoint')}>
            <CopyableRouteEndpointUrl path={inbound.path} port={row.localAuth.port} host={inbound.host} endpointId={inbound.endpointId} brandAgentId={localBrand} />
          </Field>
          <Field label={t('routes.trace.detail.code')}>{row.localEndpoint?.code || '—'}</Field>
          <Field label={t('routes.trace.detail.message')}>{row.localEndpoint?.message || '—'}</Field>
        </>,
      },
      {
        id: 'admission',
        title: t('routes.trace.detailStage.admission'),
        status: row.admission?.status ?? fallbackLaterStatus(row),
        summary: row.admission?.message,
        details: <>
          <Field label={t('routes.trace.detail.code')}>{row.admission?.code || '—'}</Field>
          <Field label={t('routes.trace.detail.message')}>{row.admission?.message || t('routes.trace.detail.noSeparateData')}</Field>
        </>,
      },
      {
        id: 'route_resolution',
        title: t('routes.trace.detailStage.routeResolution'),
        status: row.routeResolution?.status ?? fallbackLaterStatus(row),
        summary: model || null,
        details: <>
          <Field label={t('routes.activity.colModel')}>{model || '—'}</Field>
          <Field label={t('routes.trace.detail.code')}>{row.routeResolution?.code || '—'}</Field>
          <Field label={t('routes.trace.detail.message')}>{row.routeResolution?.message || '—'}</Field>
        </>,
      },
      {
        id: 'pool',
        title: t('routes.trace.detailStage.pool'),
        status: activityTraceStageStatus(row, 'pool'),
        summary: selectedLogin?.label,
        details: <>
          <Field label={t('routes.activity.selectedLogin')}>{selectedLogin?.label || '—'}</Field>
          <Field label={t('routes.activity.poolModel')}>{row.upstream.upstreamModel || model || '—'}</Field>
          <Field label={t('routes.activity.upstreamKey')}><span className="font-mono">{keyHint(selectedLogin?.keyLast4)}</span></Field>
          {attempts.map((attempt, index) => (
            <Field key={`${attempt.member.sourceId}-${index}`} label={t('routes.trace.attempt', { n: index + 1 })}>
              {attempt.member.label} · {activityTraceStageStatusLabel(attempt.status, t)}{attempt.code ? ` · ${attempt.code}` : ''}{attempt.message ? ` · ${attempt.message}` : ''}
            </Field>
          ))}
          <Field label={t('routes.trace.detail.code')}>{row.pool.code || '—'}</Field>
          <Field label={t('routes.trace.detail.message')}>{row.pool.message || '—'}</Field>
        </>,
      },
      {
        id: 'request_conversion',
        title: t('routes.trace.detailStage.requestConversion'),
        status: activityTraceStageStatus(row, 'conversion'),
        summary: activityTraceConversionLabel(row, t),
        details: <>
          <Field label={t('routes.trace.path')}>{row.conversion.path || '—'}</Field>
          <Field label={t('routes.trace.result')}>{row.conversion.result || '—'}</Field>
          <Field label={t('routes.trace.detail.code')}>{row.conversion.code || '—'}</Field>
          <Field label={t('routes.trace.detail.message')}>{row.conversion.message || '—'}</Field>
        </>,
      },
      {
        id: 'upstream_request',
        title: t('routes.trace.detailStage.upstreamRequest'),
        status: upstreamRequestStatus,
        summary: row.upstream.url,
        details: <>
          <Field label={t('routes.activity.outboundEndpoint')}><span className="font-mono">{row.upstream.url || '—'}</span></Field>
          <Field label={t('routes.activity.upstreamAuthLogin')}>{selectedLogin?.label || '—'}</Field>
          <Field label={t('routes.activity.upstreamKey')}><span className="font-mono">{keyHint(selectedLogin?.keyLast4)}</span></Field>
          <Field label={t('routes.activity.upstreamModel')}>{row.upstream.upstreamModel || row.upstream.model || model || '—'}</Field>
        </>,
      },
      {
        id: 'upstream_response',
        title: t('routes.trace.detailStage.upstreamResponse'),
        status: row.upstreamAuth.status === 'failed'
          ? 'failed'
          : activityTraceStageStatus(row, 'upstream'),
        summary: row.upstream.httpStatus != null ? String(row.upstream.httpStatus) : null,
        details: <>
          <Field label={t('routes.trace.detail.httpStatus')}>{row.upstream.httpStatus ?? '—'}</Field>
          <Field label={t('routes.trace.detail.authResult')}>{activityTraceStageStatusLabel(row.upstreamAuth.status, t)}{row.upstreamAuth.httpStatus != null ? ` · ${row.upstreamAuth.httpStatus}` : ''}</Field>
          <Field label={t('routes.trace.detail.code')}>{row.upstream.code || row.upstreamAuth.code || '—'}</Field>
          <Field label={t('routes.trace.detail.message')}>{row.upstream.message || row.upstreamAuth.message || '—'}</Field>
        </>,
      },
      {
        id: 'response_conversion',
        title: t('routes.trace.detailStage.responseConversion'),
        status: row.responseConversion?.status ?? fallbackLaterStatus(row),
        summary: responseResultLabel(row.responseConversion?.result, t),
        details: <>
          <Field label={t('routes.trace.path')}>{row.responseConversion?.path || '—'}</Field>
          <Field label={t('routes.trace.result')}>{responseResultLabel(row.responseConversion?.result, t)}</Field>
          <Field label={t('routes.trace.detail.code')}>{row.responseConversion?.code || '—'}</Field>
          <Field label={t('routes.trace.detail.message')}>{row.responseConversion?.message || '—'}</Field>
        </>,
      },
      {
        id: 'delivery',
        title: t('routes.trace.detailStage.delivery'),
        status: row.delivery?.status ?? fallbackLaterStatus(row),
        summary: row.delivery ? completionLabel(row.delivery.completion, t) : null,
        details: <>
          <Field label={t('routes.trace.detail.httpStatus')}>{row.delivery?.httpStatus ?? row.httpStatus}</Field>
          <Field label={t('routes.trace.detail.stream')}>{row.delivery?.stream ? t('routes.trace.detail.streaming') : t('routes.trace.detail.notStreaming')}</Field>
          <Field label={t('routes.trace.detail.completion')}>{completionLabel(row.delivery?.completion, t)}</Field>
          <Field label={t('routes.activity.colDuration')}>{duration || '—'}</Field>
          <Field label={t('routes.activity.colFirstToken')}>{firstToken || '—'}</Field>
          <Field label={t('routes.activity.colTokens')}>{tokens || '—'}</Field>
        </>,
      },
    ];
  }, [duration, firstToken, inbound, localBrand, model, row, t, tokens]);

  const toggle = (id: DetailStageId) => {
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
              <TraceStageCard key={stage.id} stage={stage} index={index} expanded={expanded.has(stage.id)} onToggle={() => toggle(stage.id)} />
            ))}
          </ol>
        </section>
      </div>
    </SideInspectPanel>
  );
}
