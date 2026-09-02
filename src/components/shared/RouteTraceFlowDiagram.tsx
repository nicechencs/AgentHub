import { Check, Minus, X } from 'lucide-react';
import { useMemo } from 'react';
import { RouteEndpointTypeText } from '@/components/shared/RouteEndpointUrl';
import { useI18n } from '@/components/shared/LanguageProvider';
import {
  buildTraceFlowView,
  TRACE_FLOW_MATRIX_COLS,
  TRACE_FLOW_MATRIX_ROWS,
  traceFlowEndpointBrandAgentId,
  traceFlowEndpointSurface,
  type TraceFlowStageState,
  type TraceFlowView,
} from '@/components/shared/route-trace-visual-model';
import type { AdapterBridgeRouteTrace } from '@/lib/backend/contracts/adapter';
import { cn } from '@/lib/utils';

export type RouteTraceFlowRow = AdapterBridgeRouteTrace & {
  sourceLabel?: string;
  legacySummary?: boolean;
  unauthenticated?: boolean;
};

function stageBorder(state: TraceFlowStageState): string {
  switch (state) {
    case 'ok':
      return 'border-success/60 shadow-[0_0_0_1px_rgba(var(--success-rgb,34,197,94),0.15)]';
    case 'failed':
      return 'border-danger/70 shadow-[0_0_0_1px_rgba(var(--danger-rgb,239,68,68),0.2)]';
    case 'active':
      return 'border-accent shadow-[0_0_12px_-2px] shadow-accent/40';
    case 'skipped':
      return 'border-border/60 opacity-50';
    default:
      return 'border-border/80 opacity-40';
  }
}

function stageBg(state: TraceFlowStageState): string {
  switch (state) {
    case 'ok':
      return 'bg-success/8';
    case 'failed':
      return 'bg-danger/8';
    case 'active':
      return 'bg-accent/10';
    case 'skipped':
      return 'bg-subtle/50';
    default:
      return 'bg-panel';
  }
}

function StatusIcon({ state }: { state: TraceFlowStageState }) {
  if (state === 'ok') return <Check className="h-3.5 w-3.5 text-success" aria-hidden />;
  if (state === 'failed') return <X className="h-3.5 w-3.5 text-danger" aria-hidden />;
  if (state === 'skipped') return <Minus className="h-3.5 w-3.5 text-muted" aria-hidden />;
  return <span className="inline-block h-2 w-2 rounded-full bg-muted" aria-hidden />;
}

function FlowArrow() {
  return (
    <div className="flex shrink-0 items-center justify-center px-1 text-muted" aria-hidden>
      <svg width="20" height="12" viewBox="0 0 20 12" className="overflow-visible">
        <path d="M0 6 H14 M10 2 L14 6 L10 10" fill="none" stroke="currentColor" strokeWidth="1.5" />
      </svg>
    </div>
  );
}

function StageShell({
  title,
  state,
  children,
  className,
}: {
  title: string;
  state: TraceFlowStageState;
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <section
      className={cn(
        'flex min-w-0 flex-col rounded-card border p-2.5 transition-colors',
        stageBorder(state),
        stageBg(state),
        className,
      )}
    >
      <header className="mb-1.5 flex items-center gap-1.5 text-meta font-medium text-primary">
        <StatusIcon state={state} />
        <span>{title}</span>
      </header>
      {children}
    </section>
  );
}

function LocalEndpointHub({ view }: { view: TraceFlowView }) {
  const { t } = useI18n();
  const port = view.legacySummary ? null : '127.0.0.1';
  return (
    <StageShell title={t('routes.trace.flow.localHost')} state={view.activeEndpoint ? 'active' : 'idle'}>
      <p className="mb-2 font-mono text-meta text-muted">{port ?? t('routes.trace.flow.localHostPending')}</p>
      <div className="grid grid-cols-2 gap-1.5">
        {view.endpoints.map((node) => {
          const lit = node.state === 'active';
          const endpointId = traceFlowEndpointSurface(node.kind);
          const brandId = traceFlowEndpointBrandAgentId(node.kind);
          return (
            <div
              key={node.kind}
              className={cn(
                'rounded-btn border px-2 py-1.5 transition-all',
                lit ? stageBorder('active') : stageBorder(node.state),
                lit ? stageBg('active') : stageBg(node.state),
              )}
              data-endpoint={node.kind}
              data-lit={lit ? 'true' : 'false'}
            >
              <RouteEndpointTypeText
                endpointId={endpointId}
                brandAgentId={brandId}
                className="block truncate font-mono text-meta font-medium"
              >
                {node.path}
              </RouteEndpointTypeText>
              <p className="mt-0.5 truncate text-caption text-secondary">
                {t(node.labelKey as Parameters<typeof t>[0])}
              </p>
            </div>
          );
        })}
      </div>
    </StageShell>
  );
}

function LocalAuthStage({ view }: { view: TraceFlowView }) {
  const { t } = useI18n();
  return (
    <StageShell title={t('routes.trace.stageLocalAuth')} state={view.localAuth.state} className="min-w-[7rem]">
      <p className="text-meta text-secondary">
        {view.localAuth.state === 'ok'
          ? t('routes.trace.flow.authPass')
          : view.localAuth.state === 'failed'
            ? t('routes.trace.flow.authFail')
            : view.localAuth.state === 'skipped'
              ? t('routes.trace.flow.stageSkipped')
              : t('routes.trace.flow.authPending')}
      </p>
      {view.localAuth.detail ? (
        <p className="mt-1 truncate font-mono text-caption text-muted">{view.localAuth.detail}</p>
      ) : null}
    </StageShell>
  );
}

function PoolStage({ view }: { view: TraceFlowView }) {
  const { t } = useI18n();
  return (
    <StageShell title={t('routes.trace.stagePool')} state={view.pool.state} className="min-w-[9rem]">
      {view.pool.members.length === 0 ? (
        <p className="text-meta text-muted">{t('routes.trace.flow.poolEmpty')}</p>
      ) : (
        <ul className="space-y-1">
          {view.pool.members.map((member) => (
            <li
              key={`${member.label}-${member.attemptIndex ?? 0}`}
              className={cn(
                'flex items-center gap-1.5 rounded-btn border px-2 py-1 text-meta',
                member.selected
                  ? cn(stageBorder(member.state), stageBg(member.state))
                  : 'border-border/60 bg-panel/50 text-muted',
              )}
            >
              <span
                className={cn(
                  'h-2 w-2 shrink-0 rounded-full',
                  member.selected ? 'bg-accent' : 'bg-border',
                )}
                aria-hidden
              />
              <span className="min-w-0 truncate">{member.label}</span>
              {member.attemptIndex != null && member.state === 'failed' ? (
                <span className="shrink-0 text-caption text-danger">
                  {t('routes.trace.attempt', { n: member.attemptIndex })}
                </span>
              ) : null}
            </li>
          ))}
        </ul>
      )}
    </StageShell>
  );
}

const MATRIX_COL_LABEL_KEYS: Record<(typeof TRACE_FLOW_MATRIX_COLS)[number], string> = {
  anthropic: 'routes.trace.flow.colAnthropic',
  openai_chat: 'routes.trace.flow.colOpenAiChat',
  codex_responses: 'routes.trace.flow.colCodex',
  grok: 'routes.trace.flow.colGrok',
};

const MATRIX_ROW_LABEL_KEYS: Record<(typeof TRACE_FLOW_MATRIX_ROWS)[number], string> = {
  messages: 'routes.trace.flow.rowMessages',
  responses: 'routes.trace.flow.rowResponses',
  chat: 'routes.trace.flow.rowChat',
};

function ConversionMatrix({ view }: { view: TraceFlowView }) {
  const { t } = useI18n();
  const stageState = view.conversion.passthrough ? view.conversion.state : view.conversion.state;
  return (
    <StageShell title={t('routes.trace.stageConversion')} state={stageState} className="min-w-[12rem]">
      {view.conversion.passthrough ? (
        <p className="mb-2 text-meta text-secondary">{t('routes.trace.flow.passthrough')}</p>
      ) : view.conversion.pathId ? (
        <p className="mb-2 truncate font-mono text-caption text-muted">{view.conversion.pathId}</p>
      ) : null}
      <div className="overflow-x-auto">
        <table className="w-full min-w-[14rem] border-collapse text-caption">
          <thead>
            <tr>
              <th className="p-0.5" aria-hidden />
              {TRACE_FLOW_MATRIX_COLS.map((col) => (
                <th key={col} className="px-0.5 pb-1 text-center font-normal text-muted">
                  {t(MATRIX_COL_LABEL_KEYS[col] as Parameters<typeof t>[0])}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {TRACE_FLOW_MATRIX_ROWS.map((row) => (
              <tr key={row}>
                <th className="pr-1 text-left font-normal text-muted">
                  {t(MATRIX_ROW_LABEL_KEYS[row] as Parameters<typeof t>[0])}
                </th>
                {TRACE_FLOW_MATRIX_COLS.map((col) => {
                  const cell = view.conversion.matrix.find((item) => item.row === row && item.col === col);
                  const state = cell?.state ?? 'idle';
                  const lit = state === 'ok' || state === 'active' || state === 'failed';
                  return (
                    <td key={col} className="p-0.5">
                      <div
                        className={cn(
                          'flex h-6 w-full items-center justify-center rounded border transition-all',
                          lit ? stageBorder(state) : 'border-border/40 bg-panel/30',
                          lit ? stageBg(state) : '',
                        )}
                        title={cell?.pathId}
                        data-matrix-cell={cell?.pathId}
                        data-lit={lit ? 'true' : 'false'}
                      >
                        {lit ? <StatusIcon state={state} /> : null}
                      </div>
                    </td>
                  );
                })}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </StageShell>
  );
}

function UpstreamAuthStage({ view }: { view: TraceFlowView }) {
  const { t } = useI18n();
  return (
    <StageShell title={t('routes.trace.stageUpstreamAuth')} state={view.upstreamAuth.state} className="min-w-[7rem]">
      <p className="text-meta text-secondary">
        {view.upstreamAuth.httpStatus != null ? `HTTP ${view.upstreamAuth.httpStatus}` : '—'}
      </p>
      {view.upstreamAuth.code ? (
        <p className="mt-1 truncate font-mono text-caption text-muted">{view.upstreamAuth.code}</p>
      ) : null}
    </StageShell>
  );
}

function UpstreamStage({ view }: { view: TraceFlowView }) {
  const { t } = useI18n();
  return (
    <StageShell title={t('routes.trace.stageUpstream')} state={view.upstream.state} className="min-w-[9rem]">
      {view.upstream.url ? (
        <p className="truncate font-mono text-meta text-secondary">{view.upstream.url}</p>
      ) : (
        <p className="text-meta text-muted">—</p>
      )}
      {view.upstream.accountLabel ? (
        <p className="mt-1 truncate text-meta">
          {t('routes.trace.account')}: {view.upstream.accountLabel}
        </p>
      ) : null}
      {view.upstream.upstreamModel ? (
        <p className="mt-0.5 truncate text-caption text-muted">
          {t('routes.trace.upstreamModel')}: {view.upstream.upstreamModel}
        </p>
      ) : null}
      {view.upstream.httpStatus != null ? (
        <p className={cn(
          'mt-1 text-meta font-medium',
          view.upstream.state === 'ok' ? 'text-success' : view.upstream.state === 'failed' ? 'text-danger' : 'text-secondary',
        )}
        >
          {view.upstream.state === 'ok'
            ? t('routes.trace.flow.upstreamOk', { status: view.upstream.httpStatus })
            : view.upstream.state === 'failed'
              ? t('routes.trace.flow.upstreamFail', { status: view.upstream.httpStatus })
              : `HTTP ${view.upstream.httpStatus}`}
        </p>
      ) : null}
    </StageShell>
  );
}

/**
 * Visual request-flow diagram for route monitoring (endpoints → auth → pool → matrix → upstream).
 */
export function RouteTraceFlowDiagram({
  row,
  className,
  compact,
}: {
  row: RouteTraceFlowRow;
  className?: string;
  /** Static legend preview without trace-specific highlights. */
  compact?: boolean;
}) {
  const { t } = useI18n();
  const view = useMemo(() => buildTraceFlowView(row), [row]);

  if (compact) {
    return (
      <div
        className={cn(
          'flex flex-wrap items-stretch gap-2 rounded-card border border-border bg-panel/50 p-2',
          className,
        )}
        data-route-trace-flow-legend
      >
        <LocalEndpointHub view={{
          ...view,
          activeEndpoint: null,
          endpoints: view.endpoints.map((node) => ({ ...node, state: 'idle' as const })),
        }}
        />
        <FlowArrow />
        <LocalAuthStage view={{ ...view, localAuth: { state: 'idle' } }} />
        <FlowArrow />
        <PoolStage view={{ ...view, pool: { ...view.pool, state: 'idle', members: [] } }} />
        <FlowArrow />
        <ConversionMatrix view={{
          ...view,
          conversion: {
            ...view.conversion,
            state: 'idle',
            pathId: null,
            passthrough: false,
            matrix: view.conversion.matrix.map((cell) => ({ ...cell, state: 'idle' as const })),
          },
        }}
        />
        <FlowArrow />
        <UpstreamAuthStage view={{ ...view, upstreamAuth: { state: 'idle' } }} />
        <FlowArrow />
        <UpstreamStage view={{
          ...view,
          upstream: { state: 'idle', url: null, accountLabel: null, upstreamModel: null },
        }}
        />
      </div>
    );
  }

  return (
    <div
      className={cn('space-y-2', className)}
      data-route-trace-flow
      aria-label={t('routes.trace.pipelineAria')}
    >
      {view.legacySummary ? (
        <p className="text-meta text-muted">{t('routes.trace.legacySummary')}</p>
      ) : null}
      <div className="flex flex-wrap items-stretch gap-y-2 overflow-x-auto pb-1">
        <LocalEndpointHub view={view} />
        <FlowArrow />
        <LocalAuthStage view={view} />
        <FlowArrow />
        <PoolStage view={view} />
        <FlowArrow />
        <ConversionMatrix view={view} />
        <FlowArrow />
        <UpstreamAuthStage view={view} />
        <FlowArrow />
        <UpstreamStage view={view} />
      </div>
    </div>
  );
}
