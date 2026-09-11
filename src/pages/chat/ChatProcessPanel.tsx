import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type MouseEvent as ReactMouseEvent,
  type Ref,
} from 'react';
import { Check, Copy } from 'lucide-react';
import { AgentThinking } from '@/components/shared/AgentThinking';
import { SourcePreview } from '@/components/shared/SourcePreview';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
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
