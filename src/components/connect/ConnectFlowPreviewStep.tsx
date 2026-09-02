import * as React from 'react';
import { AlertTriangle, CheckCircle2 } from 'lucide-react';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Notice } from '@/components/shared/Notice';
import type { SwitchPreview } from '@/lib/types';
import type { ConnectFlowDeps, SourceOption } from '@/lib/connect-flow/types';
import { Skeleton } from '@/components/ui/skeleton';
import {
  describePlanPreview,
  formatConnectFlowError,
  type ConnectFlowState,
} from './connect-flow-state';

function SwitchPreviewFacts({ preview }: { preview: SwitchPreview }) {
  const { t } = useI18n();
  return (
    <div className="flex flex-col gap-2.5 text-sm">
      <div className="flex items-start gap-2">
        <CheckCircle2 className="mt-0.5 h-4 w-4 shrink-0 text-success" />
        <span className="text-secondary">{preview.backfillSummary}</span>
      </div>
      <div className="flex items-start gap-2">
        <CheckCircle2 className="mt-0.5 h-4 w-4 shrink-0 text-success" />
        <span className="text-secondary">
          {t('connect.preview.backupTo', { path: preview.backupPath })}
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
  const { t } = useI18n();
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
        setPreviewError(formatConnectFlowError(error, t));
        setPhase('error');
      },
    );
    return () => {
      cancelled = true;
    };
  }, [shouldFetch, option, previewNative, t]);

  const nativeHint = (
    <p className="text-xs text-secondary">{t('connect.preview.nativeHint')}</p>
  );
  const switchPrompt = (
    <p>{t('connect.preview.switchTo', { label: label ?? '' })}</p>
  );

  if (phase === 'loading') {
    return (
      <div className="space-y-2 text-sm">
        {switchPrompt}
        <Skeleton className="h-4 w-full" />
        <Skeleton className="h-4 w-3/4" />
        <p className="text-xs text-muted">{t('connect.preview.previewing')}</p>
        {nativeHint}
      </div>
    );
  }

  if (phase === 'error') {
    return (
      <div className="space-y-2 text-sm">
        {switchPrompt}
        <Notice tone="danger">{previewError}</Notice>
        {nativeHint}
      </div>
    );
  }

  return (
    <div className="space-y-2 text-sm">
      {switchPrompt}
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
  showImportHint,
}: {
  state: ConnectFlowState;
  option: SourceOption | null;
  previewInvalid: boolean;
  previewNative?: ConnectFlowDeps['previewNative'];
  onGoImport: () => void;
  showImportHint: boolean;
}) {
  const { t } = useI18n();
  if (previewInvalid) {
    return <Notice tone="warning">{t('connect.preview.stale')}</Notice>;
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
    return <Notice tone="warning">{t('connect.preview.noPlan')}</Notice>;
  }

  const view = describePlanPreview(state.boundPlan.plan, t);
  return (
    <div className="space-y-3 text-sm">
      <div>
        {view.title ? (
          <div className="flex flex-wrap items-center gap-2">
            <h3 className="font-medium">{view.title}</h3>
          </div>
        ) : null}
        {view.reason ? (
          <p className={view.title ? 'mt-1 text-secondary' : 'text-secondary'}>{view.reason}</p>
        ) : null}
      </div>
      {view.notes.length > 0 ? (
        <ul className="list-disc space-y-0.5 pl-5 text-secondary">
          {view.notes.map((line) => <li key={line}>{line}</li>)}
        </ul>
      ) : null}
      {state.lastError ? <Notice tone="danger">{state.lastError}</Notice> : null}
      {showImportHint ? (
        <p className="text-xs text-muted">
          {t('connect.preview.importHintBefore')}
          <button type="button" className="underline" onClick={onGoImport}>{t('connect.preview.importHintLink')}</button>
          {t('connect.preview.importHintAfter')}
        </p>
      ) : null}
    </div>
  );
}
