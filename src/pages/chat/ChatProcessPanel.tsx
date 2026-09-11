import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { AgentThinking } from '@/components/shared/AgentThinking';
import { SourcePreview } from '@/components/shared/SourcePreview';
import { useI18n } from '@/components/shared/LanguageProvider';
import {
  formatToolStep,
  isProtocolProcessStep,
  phaseFromMessageStatus,
  stepSummary,
  timelineProcessSteps,
  toolActionTone,
  type AgentProcessView,
} from '@/lib/chat-process';
import { looksLikeJsonObject } from '@/lib/source-preview';
import type { ProcessStep } from '@/lib/types';
import { cn } from '@/lib/utils';
import {
  clipProcessTail,
  formatStepInput,
  isProcessActivePhase,
  pinElementScrollToBottom,
  thinkingChromeLabel,
} from './chat-format';

function looksLikeDiff(text: string): boolean {
  return (
    /^(?:diff --git|@@ |--- |\+\+\+ )/m.test(text) ||
    (text.includes('\n+') && text.includes('\n-') && /^(?:[+-](?![+-])).+/m.test(text))
  );
}

function PayloadPreview({
  text,
  density,
  className,
}: {
  text: string;
  density: 'preview' | 'compact';
  className?: string;
}) {
  if (looksLikeJsonObject(text)) {
    return (
      <SourcePreview
        value={text}
        format="json"
        density={density}
        showCopy
        className={className}
      />
    );
  }
  if (looksLikeDiff(text)) {
    return <DiffAwarePre text={text} className={className} />;
  }
  return <pre className={className}>{clipProcessTail(text)}</pre>;
}

/** Render tool/stderr text; highlight unified-diff style lines when present. */
function DiffAwarePre({ text, className }: { text: string; className?: string }) {
  const { t } = useI18n();

  const lines = text.split('\n').slice(0, 200);
  return (
    <pre className={cn(className, 'space-y-0')}>
      {lines.map((line, i) => {
        const tone =
          line.startsWith('+') && !line.startsWith('+++')
            ? 'text-success'
            : line.startsWith('-') && !line.startsWith('---')
              ? 'text-danger'
              : line.startsWith('@@')
                ? 'text-info'
                : 'text-secondary';
        return (
          <div key={i} className={cn('whitespace-pre-wrap break-all', tone)}>
            {line || ' '}
          </div>
        );
      })}
      {text.split('\n').length > 200 ? (
        <div className="text-muted">{t('chat.process.truncated')}</div>
      ) : null}
    </pre>
  );
}

function toolHasProtocolDetails(step: Extract<ProcessStep, { type: 'tool' }>): boolean {
  return Boolean(
    step.name ||
      step.status ||
      formatStepInput(step.input) ||
      (step.result && step.result.trim()),
  );
}

function ProcessStepRow({ step }: { step: ProcessStep }) {
  const { t } = useI18n();
  if (step.type === 'tool') {
    const input = formatStepInput(step.input);
    const live = toolActionTone(step.status) === 'live';
    return (
      <div className="py-1">
        <div
          className={cn(
            'font-medium text-secondary',
            live && 'agent-progress-running',
          )}
        >
          {formatToolStep(step, t)}
        </div>
        {toolHasProtocolDetails(step) ? (
          <details className="mt-0.5 text-meta" onClick={(e) => e.stopPropagation()}>
            <summary className="cursor-pointer text-muted">{t('chat.process.details')}</summary>
            <div className="mt-1 space-y-1">
              <div className="text-muted">
                {step.name}
                {step.status ? ` · ${step.status}` : ''}
              </div>
              {input ? <PayloadPreview text={input} density="compact" /> : null}
              {step.result ? (
                <PayloadPreview text={step.result} density="compact" className="mt-1" />
              ) : null}
            </div>
          </details>
        ) : null}
      </div>
    );
  }
  if (step.type === 'thinking') {
    return <ThinkingStepRow text={step.text} done={Boolean(step.done)} defaultOpen />;
  }
  if (step.type === 'error') {
    return <div className="py-1 text-danger">{step.message}</div>;
  }
  if (step.type === 'usage') {
    return null;
  }
  if (step.type === 'raw') {
    const body = step.text?.trim();
    return (
      <div className="py-1">
        <div className="text-muted">{stepSummary(step, t)}</div>
        {body ? (
          <details className="mt-0.5 text-meta" onClick={(e) => e.stopPropagation()}>
            <summary className="cursor-pointer text-muted">{t('chat.process.details')}</summary>
            <pre className="mt-1 max-h-24 overflow-auto whitespace-pre-wrap break-all text-muted">
              {clipProcessTail(body)}
            </pre>
          </details>
        ) : null}
      </div>
    );
  }
  return <div className="py-1 text-muted">{stepSummary(step, t)}</div>;
}

function ThinkingStepRow({
  text,
  done,
  defaultOpen,
}: {
  text: string;
  done: boolean;
  defaultOpen: boolean;
}) {
  const { t } = useI18n();
  const [elapsedMs, setElapsedMs] = useState(0);
  const startRef = useRef(Date.now());
  const [open, setOpen] = useState(defaultOpen);

  useEffect(() => {
    if (done) {
      if (!defaultOpen) setOpen(false);
      return;
    }
    setOpen(true);
    startRef.current = Date.now();
    const tick = () => setElapsedMs(Math.max(0, Date.now() - startRef.current));
    tick();
    const id = window.setInterval(tick, 1000);
    return () => window.clearInterval(id);
  }, [done, defaultOpen]);

  const label = thinkingChromeLabel(done, elapsedMs, t);

  const body = clipProcessTail(text);

  return (
    <details
      className="py-1"
      open={open}
      onToggle={(e) => {
        e.stopPropagation();
        const next = e.currentTarget.open;
        if (next !== open) setOpen(next);
      }}
    >
      <summary className="cursor-pointer list-none text-secondary marker:content-none [&::-webkit-details-marker]:hidden">
        {done ? (
          label
        ) : (
          <AgentThinking label={label} showTimer={false} />
        )}
      </summary>
      {body ? (
        <div className="mt-0.5 whitespace-pre-wrap break-words italic text-muted">{body}</div>
      ) : null}
    </details>
  );
}

/**
 * Inspect-pane body: thinking, tools, run details. Usage stays off this surface.
 * messageStatus wins over process.phase when the turn has already ended.
 */
export function ChatProcessPanel({
  view,
  messageStatus,
  exitCode,
}: {
  view: AgentProcessView;
  messageStatus?: string;
  exitCode?: number | null;
}) {
  const { t } = useI18n();
  const timeline = timelineProcessSteps(view.steps);
  const protocolSteps = view.steps.filter(isProtocolProcessStep);

  const effectivePhase: AgentProcessView['phase'] =
    messageStatus && messageStatus !== 'running'
      ? phaseFromMessageStatus(messageStatus)
      : view.phase;

  const hasRunDetails = Boolean(
    view.command || view.stderr || exitCode != null || protocolSteps.length > 0,
  );
  const timelineRef = useRef<HTMLDivElement>(null);
  const stderrRef = useRef<HTMLPreElement>(null);

  useLayoutEffect(() => {
    pinElementScrollToBottom(timelineRef.current);
  }, [view.steps]);

  useLayoutEffect(() => {
    pinElementScrollToBottom(stderrRef.current);
  }, [view.stderr]);

  return (
    <div className="min-h-0 flex-1 space-y-2 overflow-auto text-meta text-secondary">
      {timeline.length > 0 ? (
        <div
          ref={timelineRef}
          className="space-y-0 [overflow-anchor:none] border-l border-border pl-3"
        >
          {timeline.map((step, i) => (
            <ProcessStepRow key={`${step.type}-${i}`} step={step} />
          ))}
        </div>
      ) : isProcessActivePhase(effectivePhase) ? (
        <p className="text-muted">
          {view.stdout.trim() ? t('chat.process.streamingText') : t('chat.process.waitingLogs')}
        </p>
      ) : null}
      {hasRunDetails ? (
        <details className="text-meta" open={timeline.length === 0}>
          <summary className="cursor-pointer text-muted">{t('chat.process.runDetails')}</summary>
          <div className="mt-1.5 space-y-2">
            {protocolSteps.map((step, i) => (
              <div key={`protocol-${i}`} className="text-muted">
                {stepSummary(step, t)}
              </div>
            ))}
            {view.command ? (
              <div>
                <div className="mb-0.5 text-muted">{t('chat.process.command')}</div>
                <pre className="max-h-24 overflow-auto whitespace-pre-wrap break-all rounded-card bg-subtle px-2 py-1.5 font-mono text-meta leading-relaxed text-primary">
                  {view.command}
                </pre>
              </div>
            ) : null}
            {view.stderr ? (
              <div>
                <div className="mb-0.5 text-muted">{t('chat.process.stderr')}</div>
                <pre
                  ref={stderrRef}
                  className="max-h-36 overflow-auto [overflow-anchor:none] whitespace-pre-wrap break-all rounded-card bg-subtle px-2 py-1.5 font-mono text-meta leading-relaxed text-danger/90"
                >
                  {view.stderr}
                </pre>
              </div>
            ) : null}
            {exitCode != null ? (
              <div className="text-muted">{t('chat.process.exitCode', { code: exitCode })}</div>
            ) : null}
          </div>
        </details>
      ) : null}
    </div>
  );
}
