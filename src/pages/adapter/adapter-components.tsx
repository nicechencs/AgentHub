import { useState, type ReactNode } from 'react';
import { ChevronDown, ExternalLink } from 'lucide-react';
import { ErrorState } from '@/components/shared/ErrorState';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Skeleton } from '@/components/ui/skeleton';
import { openExternalLink } from '@/lib/open-external';
import { cn } from '@/lib/utils';
import type {
  AdapterApplyPlan,
  AdapterBridgeRuntimeStatus,
  AdapterRouteAnalysis,
} from '@/lib/backend/contracts/adapter';
import { AdapterProfilesList, type AdapterProfilesListProps } from './AdapterProfilesList';
import { AdapterRoutePipeline } from './AdapterRoutePipeline';
import {
  adapterActionLabel,
  adapterErrorDetails,
  adapterErrorRetryHint,
  adapterPlanChangeLabel,
  adapterPreviewOutcome,
  adapterServiceImpactLabel,
  canApplyAdapterPlan,
  errorMessage,
  unsupportedPresentation,
} from './adapter-model';
import type { AdapterPipelineModel } from './adapter-view-model';
import { oauthIncompleteAuthHint } from './adapter-sources';

export function AdapterErrorLines({
  error,
  fallback,
}: {
  error: unknown;
  fallback: string;
}) {
  const details = adapterErrorDetails(error);
  const retryHint = adapterErrorRetryHint(error);
  return (
    <>
      <p className="text-sm text-danger" role="alert">{errorMessage(error, fallback)}</p>
      {details ? <p className="text-xs text-secondary">{details}</p> : null}
      {retryHint ? <p className="text-xs text-secondary">{retryHint}</p> : null}
    </>
  );
}

/** A degraded bridge still owns its local listener and must be stopped, not started again. */
export function isBridgeStopCapable(
  state: AdapterBridgeRuntimeStatus['state'] | undefined,
): boolean {
  return state === 'running' || state === 'degraded';
}

/** Small injectable seam that keeps the Adapter evidence path on the Tauri-safe opener. */
export async function openAdapterEvidence(
  url: string,
  opener: (target: string) => Promise<void> = openExternalLink,
): Promise<void> {
  await opener(url);
}

export function AdapterPreviewResult({
  analysis,
  plan,
  loading,
  error,
  onRetry,
  compact = false,
  onApply,
  applyError,
  authIncomplete = false,
  authHint,
  pipeline,
}: {
  analysis: AdapterRouteAnalysis | null;
  plan: AdapterApplyPlan | null;
  loading: boolean;
  error: unknown;
  onRetry: () => void;
  compact?: boolean;
  onApply?: () => void;
  applyError?: unknown;
  authIncomplete?: boolean;
  authHint?: string;
  /** Optional route topology rendered above the conclusion (preview pane only). */
  pipeline?: AdapterPipelineModel | null;
}) {
  if (loading) {
    return (
      <div className="space-y-2" aria-live="polite">
        <p className="text-sm text-secondary">分析中…</p>
        <Skeleton className="h-4 w-32" />
        <Skeleton className="h-4 w-48" />
      </div>
    );
  }
  if (error) {
    return (
      <ErrorState
        compact={compact}
        error={errorMessage(error, '无法分析此连接')}
        title="分析失败"
        onRetry={onRetry}
      />
    );
  }
  if (!analysis) return <p className="text-sm text-secondary">选择来源后显示结果。</p>;

  if (analysis.route === 'unsupported') {
    // Unsupported is a neutral gate conclusion — never a red fault, never Apply/Bridge.
    const presentation = unsupportedPresentation(analysis, plan);
    return (
      <div className="space-y-3 text-sm">
        {pipeline ? <AdapterRoutePipeline model={pipeline} /> : null}
        <PreviewHeader
          title={presentation.headline}
          badgeLabel={presentation.badgeLabel}
          badgeVariant="default"
          summary={presentation.summary}
        />
        {presentation.reason ? (
          <p className="text-secondary">{presentation.reason}</p>
        ) : null}
        {presentation.alternatives.length > 0 ? (
          <ul className="list-disc space-y-0.5 pl-5 text-secondary">
            {presentation.alternatives.map((line) => (
              <li key={line}>{line}</li>
            ))}
          </ul>
        ) : null}
        <PreviewDetails>
          <StringList title="说明" values={presentation.gateLines} empty="" />
          {presentation.safetyNote ? (
            <p className="text-xs text-muted">{presentation.safetyNote}</p>
          ) : null}
          {analysis.limitations.length > 0 ? (
            <StringList title="限制" values={analysis.limitations} empty="" />
          ) : null}
          <EvidenceList evidence={analysis.evidence} />
        </PreviewDetails>
      </div>
    );
  }

  const canApply = canApplyAdapterPlan(plan) && !authIncomplete;
  const outcome = adapterPreviewOutcome({
    route: analysis.route,
    canApply: canApplyAdapterPlan(plan),
    authIncomplete,
  });
  // Only the backend canApply gate may surface mutation controls.
  const showApply = Boolean(onApply) && canApply;
  const changes = plan?.changes ?? [];
  const hasDetails = changes.length > 0
    || analysis.actions.length > 0
    || analysis.limitations.length > 0
    || analysis.evidence.length > 0
    || Boolean(plan?.serviceImpact);

  return (
    <div className="space-y-3 text-sm">
      {pipeline ? <AdapterRoutePipeline model={pipeline} /> : null}
      <PreviewHeader
        title={outcome.title}
        badgeLabel={outcome.badgeLabel}
        badgeVariant={outcome.badgeVariant}
        summary={outcome.nextStep}
      />
      {authIncomplete && (
        <p className="text-sm text-warning" role="status">
          {authHint ?? oauthIncompleteAuthHint()}{' '}
          <a className="underline" href="#/connections">去 Connections</a>
        </p>
      )}
      {showApply && (
        <Button onClick={onApply}>
          {analysis.route === 'local_bridge' ? '启用本地桥接' : '应用配置'}
        </Button>
      )}
      {applyError ? <AdapterErrorLines error={applyError} fallback="应用适配失败" /> : null}
      {hasDetails ? (
        <PreviewDetails>
          <AdapterPreviewList title="预计改动" values={changes} empty="无需写入配置。" />
          <p className="text-xs text-secondary">
            运行方式：{adapterServiceImpactLabel(plan?.serviceImpact)}
          </p>
          {analysis.actions.length > 0 ? (
            <AdapterActionList actions={analysis.actions} />
          ) : null}
          {analysis.limitations.length > 0 ? (
            <StringList title="限制" values={analysis.limitations} empty="" />
          ) : null}
          {analysis.evidence.length > 0 ? (
            <EvidenceList evidence={analysis.evidence} />
          ) : null}
        </PreviewDetails>
      ) : null}
    </div>
  );
}

function PreviewHeader({
  title,
  badgeLabel,
  badgeVariant,
  summary,
}: {
  title: string;
  badgeLabel: string;
  badgeVariant: 'success' | 'warning' | 'default' | 'info';
  summary: string;
}) {
  return (
    <div className="space-y-1">
      <div className="flex flex-wrap items-center gap-2">
        <h2 className="text-base font-medium">{title}</h2>
        <Badge variant={badgeVariant}>{badgeLabel}</Badge>
      </div>
      <p className="text-secondary">{summary}</p>
    </div>
  );
}

/** Secondary detail blocks stay collapsed so the first screen stays scannable. */
function PreviewDetails({ children }: { children: ReactNode }) {
  return (
    <details className="group rounded-btn border border-border bg-subtle/60">
      <summary className="flex cursor-pointer list-none items-center justify-between gap-2 px-3 py-2 text-xs font-medium text-secondary marker:content-none [&::-webkit-details-marker]:hidden">
        <span>查看详情</span>
        <ChevronDown className="h-3.5 w-3.5 shrink-0 transition-transform group-open:rotate-180" />
      </summary>
      <div className={cn('space-y-3 border-t border-border px-3 py-3')}>
        {children}
      </div>
    </details>
  );
}

/** Stable page-facing alias for the managed-profile service list. */
export function AdapterProfiles(props: AdapterProfilesListProps) {
  return <AdapterProfilesList {...props} />;
}

export function AdapterPreviewList({
  title,
  values,
  empty,
}: {
  title: string;
  values: AdapterApplyPlan['changes'];
  empty: string;
}) {
  return (
    <section>
      <h3 className="font-medium">{title}</h3>
      {values.length ? (
        <ul className="mt-1 list-disc space-y-1 pl-5 text-secondary">
          {values.map((change) => (
            <li key={`${change.target}-${change.field}`}>
              {adapterPlanChangeLabel(change)}
            </li>
          ))}
        </ul>
      ) : <p className="mt-1 text-secondary">{empty}</p>}
    </section>
  );
}

function AdapterActionList({ actions }: Pick<AdapterRouteAnalysis, 'actions'>) {
  return (
    <section>
      <h3 className="font-medium">步骤</h3>
      {actions.length ? (
        <ul className="mt-1 list-disc space-y-1 pl-5 text-secondary">
          {actions.map((item) => (
            <li key={`${item.kind}-${item.target}-${item.description}`}>
              {adapterActionLabel(item)}
            </li>
          ))}
        </ul>
      ) : <p className="mt-1 text-secondary">无额外步骤。</p>}
    </section>
  );
}

function StringList({ title, values, empty }: { title: string; values: string[]; empty: string }) {
  return (
    <section>
      <h3 className="font-medium">{title}</h3>
      {values.length ? (
        <ul className="mt-1 list-disc space-y-1 pl-5 text-secondary">
          {values.map((value) => <li key={value}>{value}</li>)}
        </ul>
      ) : <p className="mt-1 text-secondary">{empty}</p>}
    </section>
  );
}

function EvidenceList({ evidence }: Pick<AdapterRouteAnalysis, 'evidence'>) {
  const [openError, setOpenError] = useState<unknown>(null);

  const openEvidence = async (url: string) => {
    setOpenError(null);
    try {
      await openAdapterEvidence(url);
    } catch (error) {
      setOpenError(error);
    }
  };

  if (evidence.length === 0) return null;

  return (
    <section>
      <h3 className="font-medium">参考</h3>
      <ul className="mt-1 space-y-1 text-secondary">
        {evidence.map((item) => (
          <li key={item.url}>
            <button
              type="button"
              className="inline-flex items-center gap-1 text-info hover:underline"
              onClick={() => { void openEvidence(item.url); }}
            >
              {item.label} <ExternalLink className="h-3 w-3" />
            </button>
            <span className="ml-1 text-xs text-muted">{item.verifiedAt}</span>
          </li>
        ))}
      </ul>
      {openError ? (
        <p className="mt-2 text-sm text-danger" role="alert">
          {errorMessage(openError, '无法打开外部链接')}
        </p>
      ) : null}
    </section>
  );
}
