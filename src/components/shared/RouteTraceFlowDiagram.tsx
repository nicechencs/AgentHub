import { Check, Minus, X } from 'lucide-react';
import { useMemo, type ReactNode } from 'react';
import { RouteEndpointTypeText } from '@/components/shared/RouteEndpointUrl';
import { Hint, Tip } from '@/components/ui/tooltip';
import { Card } from '@/components/ui/card';
import { useI18n } from '@/components/shared/LanguageProvider';
import type { TranslateFn } from '@/lib/i18n';
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
    <div
      className="flex shrink-0 items-center justify-center px-1 text-muted"
      aria-hidden
    >
      <svg width="20" height="12" viewBox="0 0 20 12" className="overflow-visible">
        <path d="M0 6 H14 M10 2 L14 6 L10 10" fill="none" stroke="currentColor" strokeWidth="1.5" />
      </svg>
    </div>
  );
}

function AuthOkMark({ state }: { state: TraceFlowStageState }) {
  const { t } = useI18n();
  if (state === 'ok') {
    return (
      <span className="text-body font-medium text-success" data-auth-ok="true">
        {t('routes.trace.flow.authOk')}
      </span>
    );
  }
  if (state === 'failed') return <X className="h-4 w-4 text-danger" aria-hidden />;
  return null;
}

function supportTip(intro: string, items: readonly string[]): ReactNode {
  if (items.length === 0) return intro;
  return (
    <div>
      <p>{intro}</p>
      <ul className="mt-1 space-y-0.5">
        {items.map((item) => (
          <li key={item} className="font-mono text-meta">{item}</li>
        ))}
      </ul>
    </div>
  );
}

function StageCard({
  title,
  state,
  stageId,
  tip,
  support,
  children,
}: {
  title: string;
  state: TraceFlowStageState;
  stageId: string;
  tip?: ReactNode;
  support?: readonly string[];
  children?: ReactNode;
}) {
  const { t } = useI18n();
  const card = (
    <Card
      className={cn(
        'flex h-full min-h-[5.5rem] min-w-0 flex-col p-3 transition-colors hover:border-accent/40 hover:bg-hover/40',
        state === 'failed' && 'border-danger/50',
        state === 'active' && 'border-accent bg-hover/40',
        state === 'ok' && 'border-success/40',
      )}
      data-stage-box={stageId}
      data-stage-state={state}
      data-stage-support={support && support.length > 0 ? support.join('\n') : undefined}
    >
      <p className="text-xs font-medium text-primary">{title}</p>
      <div className="mt-1 min-w-0 flex-1">
        {children}
        {state === 'failed' ? (
          <p className="mt-1 text-xs font-medium text-danger" data-stage-result="failed">
            {t('routes.inbound.fail')}
          </p>
        ) : null}
        {state === 'skipped' ? (
          <p className="mt-1 text-xs text-muted" data-stage-result="not-reached">
            {t('routes.trace.notReached')}
          </p>
        ) : null}
      </div>
    </Card>
  );
  if (tip == null || tip === false || tip === '') return card;
  return (
    <Hint label={tip} delayDuration={0}>
      <div className="h-full min-w-0">{card}</div>
    </Hint>
  );
}

function StageShell({
  title,
  state,
  children,
  className,
  dense,
}: {
  title: string;
  state: TraceFlowStageState;
  children?: React.ReactNode;
  className?: string;
  dense?: boolean;
}) {
  return (
    <section
      className={cn(
        'flex min-w-0 flex-col rounded-card border transition-colors',
        dense ? 'px-2 py-1.5' : 'p-2',
        stageBorder(state),
        stageBg(state),
        className,
      )}
    >
      <header className={cn(
        'flex items-center gap-1 text-caption font-medium text-primary',
        children ? 'mb-1' : null,
      )}
      >
        <StatusIcon state={state} />
        <span>{title}</span>
      </header>
      {children}
    </section>
  );
}

function LocalEndpointHub({ view }: { view: TraceFlowView }) {
  const { t } = useI18n();
  return (
    <StageShell title={t('routes.trace.flow.localHost')} state={view.activeEndpoint ? 'active' : 'idle'}>
      <div className="flex flex-col gap-1">
        {view.endpoints.map((node) => {
          const lit = node.state === 'active';
          const endpointId = traceFlowEndpointSurface(node.kind);
          const brandId = traceFlowEndpointBrandAgentId(node.kind);
          return (
            <Tip key={node.kind} label={t(node.labelKey as Parameters<typeof t>[0])}>
              <div
                className={cn(
                  'rounded-btn border px-1.5 py-1 transition-all',
                  lit ? stageBorder('active') : stageBorder(node.state),
                  lit ? stageBg('active') : stageBg(node.state),
                )}
                data-endpoint={node.kind}
                data-lit={lit ? 'true' : 'false'}
              >
                <RouteEndpointTypeText
                  endpointId={endpointId}
                  brandAgentId={brandId}
                  className="block truncate font-mono text-caption font-medium"
                >
                  {node.path}
                </RouteEndpointTypeText>
              </div>
            </Tip>
          );
        })}
      </div>
    </StageShell>
  );
}

function LocalAuthStage({ view }: { view: TraceFlowView }) {
  const { t } = useI18n();
  return (
    <StageShell
      title={t('routes.trace.stageLocalAuth')}
      state={view.localAuth.state}
      dense
      className="shrink-0"
    />
  );
}

function PoolStage({ view }: { view: TraceFlowView }) {
  const { t } = useI18n();
  return (
    <StageShell title={t('routes.trace.stagePool')} state={view.pool.state} className="min-w-[8rem] max-w-[12rem]">
      {view.pool.members.length === 0 ? (
        <p className="text-caption text-muted">{t('routes.trace.flow.poolEmpty')}</p>
      ) : (
        <ul className="space-y-0.5">
          {view.pool.members.map((member) => (
            <li
              key={`${member.label}-${member.attemptIndex ?? 0}`}
              className={cn(
                'flex items-center gap-1 truncate text-caption',
                member.selected ? stageToneText(member.state) : 'text-secondary',
              )}
            >
              <span className="min-w-0 truncate">{member.label}</span>
              {member.attemptIndex != null && member.state === 'failed' ? (
                <span className="shrink-0 text-danger">
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

function stageToneText(state: TraceFlowStageState): string {
  if (state === 'ok') return 'text-success';
  if (state === 'failed') return 'text-danger';
  return 'text-secondary';
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

function conversionCellLabel(
  row: (typeof TRACE_FLOW_MATRIX_ROWS)[number],
  col: (typeof TRACE_FLOW_MATRIX_COLS)[number],
  t: TranslateFn,
): string {
  return t('routes.trace.flow.conversionOption', {
    from: t(MATRIX_ROW_LABEL_KEYS[row] as Parameters<TranslateFn>[0]),
    to: t(MATRIX_COL_LABEL_KEYS[col] as Parameters<TranslateFn>[0]),
  });
}

function ConversionMatrixTable({ view }: { view: TraceFlowView }) {
  const { t } = useI18n();
  return (
    <table className="w-full border-collapse text-caption" data-conversion-matrix="true">
      <thead>
        <tr>
          <th className="p-0.5" aria-hidden />
          {TRACE_FLOW_MATRIX_COLS.map((col) => (
            <th key={col} className="max-w-[3.75rem] px-0.5 pb-1 text-center font-normal text-muted">
              <span className="block truncate">
                {t(MATRIX_COL_LABEL_KEYS[col] as Parameters<TranslateFn>[0])}
              </span>
            </th>
          ))}
        </tr>
      </thead>
      <tbody>
        {TRACE_FLOW_MATRIX_ROWS.map((row) => (
          <tr key={row}>
            <th className="whitespace-nowrap pr-1 text-left font-normal text-muted">
              {t(MATRIX_ROW_LABEL_KEYS[row] as Parameters<TranslateFn>[0])}
            </th>
            {TRACE_FLOW_MATRIX_COLS.map((col) => {
              const cell = view.conversion.matrix.find((item) => item.row === row && item.col === col);
              const state = cell?.state ?? 'idle';
              const lit = state === 'ok' || state === 'active' || state === 'failed';
              return (
                <td key={col} className="p-0.5">
                  <div
                    className={cn(
                      'flex h-5 w-full items-center justify-center rounded border',
                      lit ? stageBorder(state) : 'border-border/40 bg-panel/30',
                      lit ? stageBg(state) : '',
                    )}
                    aria-label={conversionCellLabel(row, col, t)}
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
  );
}

function ConversionMatrix({
  view,
  compact,
}: {
  view: TraceFlowView;
  compact?: boolean;
}) {
  const { t } = useI18n();
  const stageState = view.conversion.state;
  const option = conversionOptionLabel(view, t);
  if (compact) {
    return (
      <StageShell
        title={t('routes.trace.stageConversion')}
        state={stageState}
        dense
        className="min-w-[4.5rem] shrink-0"
      >
        {option ? (
          <p className="max-w-[7rem] truncate text-caption text-secondary">{option}</p>
        ) : null}
      </StageShell>
    );
  }
  return (
    <StageShell title={t('routes.trace.stageConversion')} state={stageState} className="min-w-[12rem]">
      {option ? (
        <p className="mb-2 truncate text-meta text-secondary">{option}</p>
      ) : null}
      <ConversionMatrixTable view={view} />
    </StageShell>
  );
}

function UpstreamAuthStage({ view }: { view: TraceFlowView }) {
  const { t } = useI18n();
  return (
    <StageShell
      title={t('routes.trace.stageUpstreamAuth')}
      state={view.upstreamAuth.state}
      dense
      className="shrink-0"
    />
  );
}

function UpstreamStage({
  view,
  urls,
}: {
  view: TraceFlowView;
  urls?: readonly string[];
}) {
  const { t } = useI18n();
  const list = urls && urls.length > 0
    ? urls
    : view.upstream.url
      ? [view.upstream.url]
      : [];
  return (
    <StageShell title={t('routes.trace.stageUpstream')} state={view.upstream.state} className="min-w-[9rem] max-w-[16rem]">
      {list.length === 0 ? (
        <p className="text-meta text-muted">—</p>
      ) : (
        <ul className="space-y-0.5">
          {list.map((url) => (
            <li key={url} className="break-all font-mono text-caption text-secondary">{url}</li>
          ))}
        </ul>
      )}
    </StageShell>
  );
}

function conversionOptionLabel(view: TraceFlowView, t: TranslateFn): string | null {
  if (view.conversion.passthrough) return t('routes.trace.flow.passthrough');
  if (view.conversion.activeRow && view.conversion.activeCol) {
    return t('routes.trace.flow.conversionOption', {
      from: t(MATRIX_ROW_LABEL_KEYS[view.conversion.activeRow] as Parameters<TranslateFn>[0]),
      to: t(MATRIX_COL_LABEL_KEYS[view.conversion.activeCol] as Parameters<TranslateFn>[0]),
    });
  }
  return view.conversion.pathId;
}

function conversionResultLabel(result: string | null, t: TranslateFn): string | null {
  if (!result) return null;
  if (result === 'converted') return t('routes.trace.flow.converted');
  if (result === 'passthrough') return t('routes.trace.flow.passthrough');
  if (result === 'failed') return t('routes.inbound.fail');
  return result;
}

function CompactPipeline({
  view,
  className,
  poolLabels,
  upstreamUrls,
}: {
  view: TraceFlowView;
  className?: string;
  poolLabels?: readonly string[];
  upstreamUrls?: readonly string[];
}) {
  const { t } = useI18n();
  const preview = view.localAuth.state === 'skipped'
    && view.pool.state === 'skipped'
    && view.conversion.state === 'skipped';
  const activeEndpoint = preview
    ? null
    : view.endpoints.find((node) => node.kind === view.activeEndpoint) ?? null;
  const endpointSupport = view.endpoints.map((node) => (
    `${node.path} · ${t(node.labelKey as Parameters<TranslateFn>[0])}`
  ));
  const poolSupport = poolLabels ?? [];
  const upstreamSupport = view.upstream.state === 'skipped' ? [] : upstreamUrls ?? [];
  const poolHit = preview ? null : view.pool.selectedLabel;
  const conversionHit = preview ? null : conversionOptionLabel(view, t);
  const conversionResult = preview
    ? null
    : conversionResultLabel(
      view.conversion.passthrough ? null : view.conversion.result,
      t,
    );

  return (
    <div
      className={cn(
        'grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-5',
        className,
      )}
      data-route-trace-flow-legend
    >
      <StageCard
        title={t('routes.trace.stageLocalAuth')}
        state={view.localAuth.state}
        stageId="local_auth"
        support={endpointSupport}
        tip={supportTip(t('routes.trace.flow.supportLocalAuth'), endpointSupport)}
      >
        {activeEndpoint ? (
          <p className="truncate font-mono text-sm font-medium text-success" data-endpoint={activeEndpoint.kind}>
            {activeEndpoint.path}
          </p>
        ) : null}
        <AuthOkMark state={view.localAuth.state} />
      </StageCard>
      <StageCard
        title={t('routes.trace.stagePool')}
        state={view.pool.state}
        stageId="pool"
        support={poolSupport}
        tip={supportTip(t('routes.trace.flow.supportPool'), poolSupport)}
      >
        {poolHit ? (
          <p className="truncate text-sm font-medium text-success">{poolHit}</p>
        ) : (
          <AuthOkMark state={view.pool.state} />
        )}
      </StageCard>
      <StageCard
        title={t('routes.trace.stageConversion')}
        state={view.conversion.state}
        stageId="conversion"
        tip={
          <div>
            <p>{t('routes.trace.flow.supportConversion')}</p>
            <div className="mt-1.5">
              <ConversionMatrixTable view={view} />
            </div>
          </div>
        }
      >
        {conversionHit ? (
          <p className="truncate text-sm font-medium text-success">{conversionHit}</p>
        ) : (
          <AuthOkMark state={view.conversion.state} />
        )}
        {conversionResult && conversionResult !== conversionHit ? (
          <p className="mt-1 truncate text-xs text-muted">{conversionResult}</p>
        ) : null}
      </StageCard>
      <StageCard
        title={t('routes.trace.stageUpstreamAuth')}
        state={view.upstreamAuth.state}
        stageId="upstream_auth"
        tip={t('routes.trace.flow.supportUpstreamAuth')}
      >
        <AuthOkMark state={view.upstreamAuth.state} />
        {view.upstreamAuth.httpStatus != null ? (
          <p className="mt-1 font-mono text-xs text-secondary">{view.upstreamAuth.httpStatus}</p>
        ) : null}
      </StageCard>
      <StageCard
        title={t('routes.trace.stageUpstream')}
        state={view.upstream.state}
        stageId="upstream"
        support={upstreamSupport}
        tip={view.upstream.state === 'skipped' ? undefined : supportTip(t('routes.trace.flow.supportUpstream'), upstreamSupport)}
      >
        {view.upstream.state !== 'skipped' && view.upstream.url ? (
          <p className="break-all font-mono text-sm font-medium text-success">{view.upstream.url}</p>
        ) : (
          <AuthOkMark state={view.upstream.state} />
        )}
      </StageCard>
    </div>
  );
}

/**
 * Visual request-flow diagram for route monitoring (endpoints → auth → pool → matrix → upstream).
 */
export function RouteTraceFlowDiagram({
  row,
  className,
  compact,
  previewPoolLabels,
  previewUpstreamUrls,
}: {
  row: RouteTraceFlowRow;
  className?: string;
  /** Five stage cards for the monitoring legend. */
  compact?: boolean;
  previewPoolLabels?: readonly string[];
  previewUpstreamUrls?: readonly string[];
}) {
  const { t } = useI18n();
  const view = useMemo(() => buildTraceFlowView(row), [row]);

  if (compact) {
    return (
      <CompactPipeline
        view={view}
        className={className}
        poolLabels={previewPoolLabels}
        upstreamUrls={previewUpstreamUrls}
      />
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
