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
import { copyTextToClipboard } from '@/components/shared/CopyTextButton';
import { AgentThinking } from '@/components/shared/AgentThinking';
import { SourcePreview } from '@/components/shared/SourcePreview';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
import { useToast } from '@/components/ui/toast';
import {
  formatToolStep,
  formatUsageStep,
  isProtocolProcessStep,
  latestThinkingStep,
  phaseFromMessageStatus,
  stepSummary,
  timelineProcessSteps,
  toolActionTarget,
  toolActionTone,
  type AgentProcessView,
} from '@/lib/chat-process';
import {
  highlightDetailLines,
  highlightSourceTokens,
  type HighlightLine,
  type SourceToken,
} from '@/components/shared/source-highlight';
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

function HeightSeparator({
  label,
  paneHeight,
  valuemin,
  onResizeStart,
  onSeparatorKeyDown,
  resetHeight,
}: {
  label: string;
  paneHeight: number;
  valuemin: number;
  onResizeStart: (e: React.PointerEvent<HTMLDivElement>) => void;
  onSeparatorKeyDown: (e: React.KeyboardEvent<HTMLDivElement>) => void;
  resetHeight: () => void;
}) {
  return (
    <div
      role="separator"
      aria-orientation="horizontal"
      aria-label={label}
      aria-valuenow={paneHeight}
      aria-valuemin={valuemin}
      tabIndex={0}
      onPointerDown={onResizeStart}
      onDoubleClick={(e) => {
        e.stopPropagation();
        resetHeight();
      }}
      onKeyDown={onSeparatorKeyDown}
      className={
        [
          'group relative z-10 h-2 shrink-0 cursor-row-resize touch-none bg-transparent outline-none',
          'after:pointer-events-none after:absolute after:inset-x-0 after:top-1/2 after:h-px after:-translate-y-1/2 after:bg-transparent after:content-[""]',
          'hover:after:bg-accent focus-visible:after:bg-accent active:after:bg-accent',
        ].join(' ')
      }
    />
  );
}

function ResizableRegion({
  pane,
  label,
  className,
  children,
}: {
  pane: ProcessLogPane;
  label: string;
  className?: string;
  children: (height: number) => ReactNode;
}) {
  const height = useProcessLogHeight(pane);
  return (
    <div className={className}>
      {children(height.paneHeight)}
      <HeightSeparator label={label} {...height} />
    </div>
  );
}

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
  const { toast } = useToast();
  const [copied, setCopied] = useState(false);
  const height = useProcessLogHeight(pane);

  const onCopy = (e: ReactMouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    if (!text.trim()) return;
    void copyTextToClipboard(text).then(
      () => {
        setCopied(true);
        window.setTimeout(() => setCopied(false), 1200);
      },
      () => toast({ title: t('common.copyFailed'), variant: 'danger' }),
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
        {pane === 'command' ? <TokenSpans text={text} format="shell" /> : text}
      </pre>
      <HeightSeparator label={resizeAria} {...height} />
    </div>
  );
}

function TokenSpans({
  text,
  format,
  fileName,
}: {
  text: string;
  format?: 'shell';
  fileName?: string | null;
}) {
  const tokens = format === 'shell' ? highlightSourceTokens(text, 'shell') : null;
  const lines = tokens ? null : highlightDetailLines(text, fileName);
  if (tokens) {
    return tokens.map((token, index) => <TokenSpan key={index} token={token} />);
  }
  if (!lines) return text;
  return <HighlightedLines lines={lines} source={text} />;
}

function HighlightedLines({
  lines,
  source,
  bodyHeight,
}: {
  lines: HighlightLine[];
  source: string;
  bodyHeight?: number;
}) {
  const { t } = useI18n();
  const { toast } = useToast();
  const [copied, setCopied] = useState(false);
  const digits = Math.max(2, String(lines.length).length);
  const onCopy = (e: ReactMouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    if (!source.trim()) return;
    void copyTextToClipboard(source).then(
      () => {
        setCopied(true);
        window.setTimeout(() => setCopied(false), 1200);
      },
      () => toast({ title: t('common.copyFailed'), variant: 'danger' }),
    );
  };
  return (
    <div className="overflow-hidden rounded-btn border border-border bg-subtle font-mono text-meta leading-[18px] text-primary">
      <div className="flex justify-end border-b border-border px-1 py-0.5">
        <Button
          type="button"
          size="sm"
          variant="ghost"
          className="h-7 shrink-0 px-2"
          aria-label={t('common.copy')}
          onClick={onCopy}
        >
          {copied ? <Check className="h-3.5 w-3.5 text-success" /> : <Copy className="h-3.5 w-3.5" />}
          {copied ? t('common.copied') : t('common.copy')}
        </Button>
      </div>
      <div className="overflow-auto" style={bodyHeight != null ? { height: bodyHeight } : undefined}>
      {lines.map((line, index) => (
        <div
          key={index}
          className={cn(
            'flex min-h-[18px]',
            line.kind === 'add' && 'chat-diff-add',
            line.kind === 'remove' && 'chat-diff-remove',
            line.kind === 'meta' && 'text-muted',
            index === 0 && 'pt-2',
            index === lines.length - 1 && 'pb-2',
          )}
        >
          <span className="chat-line-number" aria-hidden>
            {String(index + 1).padStart(digits, ' ')}
          </span>
          <div className="min-w-0 flex-1 whitespace-pre-wrap break-words pr-3">
            {line.tokens.length === 0
              ? ' '
              : line.tokens.map((token, tokenIndex) => (
                <TokenSpan key={tokenIndex} token={token} />
              ))}
          </div>
        </div>
      ))}
      </div>
    </div>
  );
}

function TokenSpan({ token }: { token: SourceToken }) {
  if (!token.className) return token.text;
  return <span className={token.className}>{token.text}</span>;
}

function PayloadPreview({
  text,
  density,
  className,
  fileName,
}: {
  text: string;
  density: 'preview' | 'compact';
  className?: string;
  fileName?: string | null;
}) {
  const { t } = useI18n();
  if (!text.trim()) return null;
  if (looksLikeJsonObject(text)) {
    if (!hasJsonPreviewContent(text)) return null;
    return (
      <ResizableRegion pane="json" label={t('chat.process.resizeJson')} className={className}>
        {(height) => (
          <SourcePreview
            value={text}
            format="json"
            density={density}
            showCopy
            bodyHeight={height}
          />
        )}
      </ResizableRegion>
    );
  }
  const clipped = clipProcessTail(text);
  const lines = highlightDetailLines(clipped, fileName);
  if (lines) {
    return (
      <ResizableRegion pane="code" label={t('chat.process.resizeCode')} className={className}>
        {(height) => <HighlightedLines lines={lines} source={clipped} bodyHeight={height} />}
      </ResizableRegion>
    );
  }
  return (
    <ResizableRegion pane="code" label={t('chat.process.resizeCode')} className={className}>
      {(height) => (
        <pre
          style={{ height }}
          className="overflow-auto whitespace-pre-wrap break-words rounded-btn border border-border bg-subtle px-3 py-2 font-mono text-meta text-primary"
        >
          {clipped}
        </pre>
      )}
    </ResizableRegion>
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
  active = false,
  thinkingStartedAt,
  thinkingDurationMs,
}: {
  step: ProcessStep;
  active?: boolean;
  thinkingStartedAt?: number;
  thinkingDurationMs?: number;
}) {
  const { t } = useI18n();
  if (step.type === 'tool') {
    const input = formatStepInput(step.input);
    const fileName = toolActionTarget(step.name, step.input);
    const live = toolActionTone(step.status) === 'live';
    return (
      <div className="py-1" data-help="chat-process-tool">
        <div
          className={cn(
            'font-medium text-primary',
            live && 'agent-progress-running',
          )}
        >
          {formatToolStep(step, t)}
        </div>
        {toolHasProtocolDetails(step) ? (
          <details className="mt-0.5 text-meta" open={active} onClick={(e) => e.stopPropagation()}>
            <summary className="cursor-pointer text-muted">{t('chat.process.details')}</summary>
            <div className="mt-1 space-y-1">
              <div className="text-muted">
                {step.name}
                {step.status ? ` · ${step.status}` : ''}
              </div>
              {input ? <PayloadPreview text={input} density="compact" fileName={fileName} /> : null}
              {step.result ? (
                <PayloadPreview text={step.result} density="compact" fileName={fileName} className="mt-1" />
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
            <ResizableRegion pane="code" label={t('chat.process.resizeCode')}>
              {(height) => (
                <pre
                  style={{ height }}
                  className="mt-1 overflow-auto whitespace-pre-wrap break-all text-primary"
                >
                  {clipProcessTail(body)}
                </pre>
              )}
            </ResizableRegion>
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
  const bodyRef = useRef<HTMLDivElement>(null);

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
      <summary className="cursor-pointer list-none text-primary marker:content-none [&::-webkit-details-marker]:hidden">
        {done ? (
          label
        ) : (
          <AgentThinking label={label} showTimer={false} className="text-meta text-primary" />
        )}
      </summary>
      {body ? (
        <ResizableRegion pane="thinking" label={t('chat.process.resizeThinking')}>
          {(height) => (
            <div
              ref={bodyRef}
              style={{ height }}
              className="mt-1 overflow-auto rounded-btn border border-border bg-subtle px-3 py-2 text-body leading-relaxed text-primary [overflow-anchor:none] [overflow-wrap:anywhere]"
            >
              {body}
            </div>
          )}
        </ResizableRegion>
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
    <div className="min-h-0 flex-1 space-y-2 overflow-auto text-meta text-primary">
      {timeline.length > 0 || userPrompt || pendingConfirm || usageSteps.length > 0 ? (
        <div
          ref={timelineRef}
          className="space-y-0 [overflow-anchor:none] border-l border-border pl-3"
        >
          {userPrompt ? (
            <div className="py-1" data-help="chat-process-user">
              <span className="text-muted">
                {t('chat.process.userSaid')}
                {' · '}
              </span>
              <span>{userPrompt}</span>
            </div>
          ) : null}
          {timelineRows.map(({ step, index, active }) => (
            <ProcessStepFrame key={processStepKey(step, index)} active={active}>
              <ProcessStepRow
                step={step}
                active={active}
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
            <div className="py-1" data-help="chat-process-confirm">
              <span className="text-muted">
                {t('chat.process.waitingConfirm')}
                {' · '}
              </span>
              <span>{pendingConfirm}</span>
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
