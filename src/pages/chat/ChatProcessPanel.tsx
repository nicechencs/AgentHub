import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type MouseEvent as ReactMouseEvent,
  type ReactNode,
  type Ref,
} from 'react';
import { Check, Copy } from 'lucide-react';
import { AgentThinking } from '@/components/shared/AgentThinking';
import { SourcePreview } from '@/components/shared/SourcePreview';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
import {
  formatToolStep,
  formatUsageStep,
  isProtocolProcessStep,
  latestThinkingStep,
  phaseFromMessageStatus,
  stepSummary,
  timelineProcessSteps,
  toolActionTone,
  type AgentProcessView,
} from '@/lib/chat-process';
import { hasJsonPreviewContent, looksLikeJsonObject } from '@/lib/source-preview';
import type { ProcessStep } from '@/lib/types';
import { cn } from '@/lib/utils';
import { processInspectStepIndex } from './chat-preview-model';
import {
  clipProcessTail,
  formatStepInput,
  isProcessActivePhase,
  pinElementScrollToBottom,
  thinkingChromeLabel,
} from './chat-format';
import type { ProcessLogPane } from './chat-process-log-model';
import { useProcessLogHeight } from './use-process-log-height';

function CopyableResizableLog({
  label,
  text,
  pane,
  resizeAria,
  preRef,
}: {
  label: string;
  text: string;
  pane: ProcessLogPane;
  resizeAria: string;
  preRef?: Ref<HTMLPreElement>;
}) {
  const { t } = useI18n();
  const [copied, setCopied] = useState(false);
  const height = useProcessLogHeight(pane);

  const onCopy = (e: ReactMouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    if (!text.trim()) return;
    void navigator.clipboard.writeText(text).then(
      () => {
        setCopied(true);
        window.setTimeout(() => setCopied(false), 1200);
      },
      () => undefined,
    );
  };

  return (
    <div>
      <div className="mb-0.5 flex items-center justify-between gap-2">
        <span className="text-muted">{label}</span>
        <Button
          type="button"
          size="sm"
          variant="ghost"
          className="h-7 shrink-0 px-2"
          aria-label={t('common.copy')}
          onClick={onCopy}
          onPointerDown={(e) => e.stopPropagation()}
        >
          {copied ? <Check className="h-3.5 w-3.5 text-success" /> : <Copy className="h-3.5 w-3.5" />}
          {copied ? t('common.copied') : t('common.copy')}
        </Button>
      </div>
      <pre
        ref={preRef}
        style={{ height: height.paneHeight }}
        className="overflow-auto [overflow-anchor:none] whitespace-pre-wrap break-all rounded-card bg-subtle px-2 py-1.5 font-mono text-meta leading-relaxed text-primary"
      >
        {text}
      </pre>
      <div
        role="separator"
        aria-orientation="horizontal"
        aria-label={resizeAria}
        aria-valuenow={height.paneHeight}
        aria-valuemin={height.valuemin}
        tabIndex={0}
        onPointerDown={height.onResizeStart}
        onDoubleClick={(e) => {
          e.stopPropagation();
          height.resetHeight();
        }}
        onKeyDown={height.onSeparatorKeyDown}
        className={
          [
            'group relative z-10 h-2 shrink-0 cursor-row-resize touch-none bg-transparent outline-none',
            'after:pointer-events-none after:absolute after:inset-x-0 after:top-1/2 after:h-px after:-translate-y-1/2 after:bg-transparent after:content-[""]',
            'hover:after:bg-accent focus-visible:after:bg-accent active:after:bg-accent',
          ].join(' ')
        }
      />
    </div>
  );
}

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
  if (!text.trim()) return null;
  if (looksLikeJsonObject(text)) {
    if (!hasJsonPreviewContent(text)) return null;
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

function processStepKey(step: ProcessStep, index: number): string {
  if (step.type === 'tool') return `tool:${step.id ?? step.name}:${index}`;
  if (step.type === 'usage') return `usage:${step.scope ?? 'turn'}:${index}`;
  if (step.type === 'thinking') return `thinking:${index}`;
  if (step.type === 'error') return `error:${index}`;
  return `${step.type}:${index}`;
}

function toolHasProtocolDetails(step: Extract<ProcessStep, { type: 'tool' }>): boolean {
  return Boolean(
    step.name ||
      step.status ||
      formatStepInput(step.input) ||
      (step.result && step.result.trim()),
  );
}

function ProcessStepFrame({
  active,
  children,
}: {
  active: boolean;
  children: ReactNode;
}) {
  return (
    <div
      data-process-step-active={active ? 'true' : undefined}
      className={cn(active && 'rounded-btn bg-hover')}
    >
      {children}
    </div>
  );
}

function ProcessStepRow({
  step,
  thinkingStartedAt,
  thinkingDurationMs,
}: {
  step: ProcessStep;
  thinkingStartedAt?: number;
  thinkingDurationMs?: number;
}) {
  const { t } = useI18n();
  if (step.type === 'tool') {
    const input = formatStepInput(step.input);
    const live = toolActionTone(step.status) === 'live';
    return (
      <div className="py-1" data-help="chat-process-tool">
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
    return (
      <ThinkingStepRow
        text={step.text}
        done={Boolean(step.done)}
        defaultOpen
        startedAt={thinkingStartedAt}
        durationMs={thinkingDurationMs}
      />
    );
  }
  if (step.type === 'error') {
    return <div className="py-1 text-danger">{step.message}</div>;
  }
  if (step.type === 'usage') {
    return <div className="py-1 text-muted">{formatUsageStep(step, t)}</div>;
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
  startedAt,
  durationMs,
}: {
  text: string;
  done: boolean;
  defaultOpen: boolean;
  startedAt?: number;
  durationMs?: number;
}) {
  const { t } = useI18n();
  const startRef = useRef(startedAt ?? Date.now());
  const [now, setNow] = useState(() => Date.now());
  const [open, setOpen] = useState(defaultOpen);
  const bodyRef = useRef<HTMLPreElement>(null);

  useEffect(() => {
    if (startedAt != null) startRef.current = startedAt;
  }, [startedAt]);

  useEffect(() => {
    if (done) {
      if (!defaultOpen) setOpen(false);
      return;
    }
    setOpen(true);
    const tick = () => setNow(Date.now());
    tick();
    const id = window.setInterval(tick, 1000);
    return () => window.clearInterval(id);
  }, [done, defaultOpen]);

  const elapsedMs = done
    ? (durationMs ?? 0)
    : Math.max(0, now - startRef.current);
  const label = thinkingChromeLabel(done, elapsedMs, t);
  const body = clipProcessTail(text);

  useLayoutEffect(() => {
    if (!done) pinElementScrollToBottom(bodyRef.current);
  }, [body, done]);

  return (
    <details
      className="py-1"
      data-help="chat-process-thinking"
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
        <pre
          ref={bodyRef}
          className="mt-0.5 max-h-40 overflow-auto [overflow-anchor:none] whitespace-pre-wrap break-words italic text-muted"
        >
          {body}
        </pre>
      ) : null}
    </details>
  );
}

/**
 * Inspect-pane body: thinking, tools, run details.
 * Usage, the user prompt, and waiting confirm hang here — not a second bus.
 * messageStatus wins over process.phase when the turn has already ended.
 */
export function ChatProcessPanel({
  view,
  messageStatus,
  exitCode,
  userPrompt,
  pendingConfirm,
  activeStepKey = null,
}: {
  view: AgentProcessView;
  messageStatus?: string;
  exitCode?: number | null;
  userPrompt?: string | null;
  pendingConfirm?: string | null;
  /** Transcript step to keep in view. The pane itself stays open across switches. */
  activeStepKey?: string | null;
}) {
  const { t } = useI18n();
  const timeline = timelineProcessSteps(view.steps);
  const protocolSteps = view.steps.filter(isProtocolProcessStep);
  const usageSteps = view.steps.filter((step): step is Extract<ProcessStep, { type: 'usage' }> => step.type === 'usage');

  const effectivePhase: AgentProcessView['phase'] =
    messageStatus && messageStatus !== 'running'
      ? phaseFromMessageStatus(messageStatus)
      : view.phase;

  const hasRunDetails = Boolean(
    view.command || view.stderr || exitCode != null || protocolSteps.length > 0,
  );
  const timelineRef = useRef<HTMLDivElement>(null);
  const stderrRef = useRef<HTMLPreElement>(null);
  const latestThinking = latestThinkingStep(timeline);
  const activeIndex = processInspectStepIndex(activeStepKey);
  let transcriptIndex = -1;
  const timelineRows = timeline.map((step, index) => {
    const inTranscript = step.type === 'thinking' || step.type === 'tool' || step.type === 'error';
    if (inTranscript) transcriptIndex += 1;
    return {
      step,
      index,
      active: inTranscript && activeIndex != null && transcriptIndex === activeIndex,
    };
  });

  useLayoutEffect(() => {
    const root = timelineRef.current;
    if (!root) return;
    const active = root.querySelector('[data-process-step-active="true"]');
    if (active instanceof HTMLElement) {
      active.scrollIntoView({ block: 'nearest' });
      return;
    }
    pinElementScrollToBottom(root);
  }, [view.steps, pendingConfirm, userPrompt, activeStepKey]);

  useLayoutEffect(() => {
    pinElementScrollToBottom(stderrRef.current);
  }, [view.stderr]);

  return (
    <div className="min-h-0 flex-1 space-y-2 overflow-auto text-meta text-secondary">
      {timeline.length > 0 || userPrompt || pendingConfirm || usageSteps.length > 0 ? (
        <div
          ref={timelineRef}
          className="space-y-0 [overflow-anchor:none] border-l border-border pl-3"
        >
          {userPrompt ? (
            <div className="py-1 text-muted" data-help="chat-process-user">
              {t('chat.process.userSaid')}
              {' · '}
              {userPrompt}
            </div>
          ) : null}
          {timelineRows.map(({ step, index, active }) => (
            <ProcessStepFrame key={processStepKey(step, index)} active={active}>
              <ProcessStepRow
                step={step}
                thinkingStartedAt={
                  step.type === 'thinking' && step === latestThinking
                    ? view.thinkingStartedAt
                    : undefined
                }
                thinkingDurationMs={
                  step.type === 'thinking' && step === latestThinking
                    ? view.thinkingDurationMs
                    : undefined
                }
              />
            </ProcessStepFrame>
          ))}
          {pendingConfirm ? (
            <div className="py-1 text-muted" data-help="chat-process-confirm">
              {t('chat.process.waitingConfirm')}
              {' · '}
              {pendingConfirm}
            </div>
          ) : null}
          {usageSteps.map((step, i) => (
            <ProcessStepRow key={processStepKey(step, i)} step={step} />
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
              <CopyableResizableLog
                label={t('chat.process.command')}
                text={view.command}
                pane="command"
                resizeAria={t('chat.process.resizeCommand')}
              />
            ) : null}
            {view.stderr ? (
              <CopyableResizableLog
                label={t('chat.process.stderr')}
                text={view.stderr}
                pane="stderr"
                resizeAria={t('chat.process.resizeLog')}
                preRef={stderrRef}
              />
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
