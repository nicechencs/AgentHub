import * as React from 'react';
import { AlertTriangle, CheckCircle2 } from 'lucide-react';
import { Notice } from '@/components/shared/Notice';
import { Badge } from '@/components/ui/badge';
import type { SwitchPreview } from '@/lib/types';
import type { ConnectFlowDeps, SourceOption } from '@/lib/connect-flow/types';
import { Skeleton } from '@/components/ui/skeleton';
import {
  describePlanPreview,
  formatConnectFlowError,
  PREVIEW_SELECTION_STALE_MESSAGE,
  type ConnectFlowState,
} from './connect-flow-state';

function SwitchPreviewFacts({ preview }: { preview: SwitchPreview }) {
  return (
    <div className="flex flex-col gap-2.5 text-sm">
      <div className="flex items-start gap-2">
        <CheckCircle2 className="mt-0.5 h-4 w-4 shrink-0 text-success" />
        <span className="text-secondary">{preview.backfillSummary}</span>
      </div>
      <div className="flex items-start gap-2">
        <CheckCircle2 className="mt-0.5 h-4 w-4 shrink-0 text-success" />
        <span className="text-secondary">
          切换前备份到 <code className="font-mono text-xs">{preview.backupPath}</code>
        </span>
      </div>
      {preview.processWarning ? (
        <div className="flex items-start gap-2">
          <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-warning" />
          <span className="text-warning">{preview.processWarning}</span>
        </div>
      ) : null}
    </div>
  );
}

function SwitchNativePreview({
  option,
  fallbackLabel,
  lastError,
  previewNative,
}: {
  option: SourceOption | null;
  fallbackLabel?: string;
  lastError: string | null;
  previewNative?: ConnectFlowDeps['previewNative'];
}) {
  const label = option?.label ?? fallbackLabel;
  const shouldFetch = Boolean(option?.ref.kind === 'provider' && previewNative);
  const [phase, setPhase] = React.useState<'idle' | 'loading' | 'ready' | 'error'>(
    shouldFetch ? 'loading' : 'idle',
  );
  const [preview, setPreview] = React.useState<SwitchPreview | null>(null);
  const [previewError, setPreviewError] = React.useState<string | null>(null);

  React.useEffect(() => {
    if (!shouldFetch || !option || !previewNative) {
      setPhase('idle');
      setPreview(null);
      setPreviewError(null);
      return;
    }
    let cancelled = false;
    setPhase('loading');
    setPreview(null);
    setPreviewError(null);
    void previewNative(option).then(
      (result) => {
        if (cancelled) return;
        setPreview(result);
        setPhase('ready');
      },
      (error: unknown) => {
        if (cancelled) return;
        setPreviewError(formatConnectFlowError(error));
        setPhase('error');
      },
    );
    return () => {
      cancelled = true;
    };
  }, [shouldFetch, option, previewNative]);

  const nativeHint = (
    <p className="text-xs text-secondary">将走本 Agent 既有切换，不会创建跨服务绑定。</p>
  );

  if (phase === 'loading') {
    return (
      <div className="space-y-2 text-sm">
        <p>切换到「{label}」？</p>
        <Skeleton className="h-4 w-full" />
        <Skeleton className="h-4 w-3/4" />
        <p className="text-xs text-muted">正在预览…</p>
        {nativeHint}
      </div>
    );
  }

  if (phase === 'error') {
    return (
      <div className="space-y-2 text-sm">
        <p>切换到「{label}」？</p>
        <Notice tone="danger">{previewError}</Notice>
        {nativeHint}
      </div>
    );
  }

  return (
    <div className="space-y-2 text-sm">
      <p>切换到「{label}」？</p>
      {preview ? <SwitchPreviewFacts preview={preview} /> : null}
      {nativeHint}
      {lastError ? <Notice tone="danger">{lastError}</Notice> : null}
    </div>
  );
}

export function ConnectFlowPreviewStep({
  state,
  option,
  previewInvalid,
  previewNative,
  onGoImport,
}: {
  state: ConnectFlowState;
  option: SourceOption | null;
  previewInvalid: boolean;
  previewNative?: ConnectFlowDeps['previewNative'];
  onGoImport: () => void;
}) {
  if (previewInvalid) {
    return <Notice tone="warning">{PREVIEW_SELECTION_STALE_MESSAGE}</Notice>;
  }

  if (state.previewKind === 'switch') {
    return (
      <SwitchNativePreview
        option={option}
        fallbackLabel={state.selectedSource?.id}
        lastError={state.lastError}
        previewNative={previewNative}
      />
    );
  }

  if (!state.boundPlan) {
    return <Notice tone="warning">没有可应用的预览，请返回重新选择。</Notice>;
  }

  const view = describePlanPreview(state.boundPlan.plan);
  return (
    <div className="space-y-3 text-sm">
      <div>
        <div className="flex flex-wrap items-center gap-2">
          <h3 className="font-medium">{view.routeLabel}</h3>
          <Badge variant="success">可应用</Badge>
        </div>
        {view.reason ? <p className="mt-1 text-secondary">{view.reason}</p> : null}
      </div>
      <section>
        <h4 className="text-xs font-medium text-secondary">将写入的配置</h4>
        {view.writes.length > 0 ? (
          <ul className="mt-1 list-disc space-y-0.5 pl-5 text-secondary">
            {view.writes.map((line) => <li key={line}>{line}</li>)}
          </ul>
        ) : (
          <p className="mt-1 text-secondary">无需写入配置。</p>
        )}
      </section>
      <section className="space-y-0.5 text-secondary">
        <p>服务影响：{view.serviceImpact}</p>
        {view.startsBridge ? <p>将启动本机路由。</p> : <p>不启动本机路由。</p>}
        {view.portNotes.map((line) => <p key={line}>端口：{line}</p>)}
        {view.modelMappings.length > 0 ? (
          <div>
            <p>模型映射：</p>
            <ul className="list-disc pl-5">
              {view.modelMappings.map((line) => <li key={line}>{line}</li>)}
            </ul>
          </div>
        ) : null}
      </section>
      {view.limitations.length > 0 ? (
        <section>
          <h4 className="text-xs font-medium text-secondary">限制</h4>
          <ul className="mt-1 list-disc pl-5 text-secondary">
            {view.limitations.map((line) => <li key={line}>{line}</li>)}
          </ul>
        </section>
      ) : null}
      {state.lastError ? <Notice tone="danger">{state.lastError}</Notice> : null}
      <p className="text-xs text-muted">
        若来源尚未登录，请先在官方 CLI 完成登录再{' '}
        <button type="button" className="underline" onClick={onGoImport}>去 Connections 导入</button>
        。
      </p>
    </div>
  );
}
