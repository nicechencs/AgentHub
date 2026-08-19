import { useLayoutEffect, useRef, useState } from 'react';
import { useI18n } from '@/components/shared/LanguageProvider';
import {
  phaseFromMessageStatus,
  processPhaseLabel,
  stepSummary,
  type AgentProcessView,
} from '@/lib/chat-process';
import type { TranslateFn } from '@/lib/i18n';
import type { ProcessStep } from '@/lib/types';
import { cn } from '@/lib/utils';
import {
  formatDurationMs,
  formatStepInput,
  isProcessActivePhase,
  isProcessErrorPhase,
} from './chat-format';

/** Render tool/stderr text; highlight unified-diff style lines when present. */
function DiffAwarePre({ text, className }: { text: string; className?: string }) {
  const { t } = useI18n();
  const looksDiff =
    /^(?:diff --git|@@ |--- |\+\+\+ )/m.test(text) ||
    (text.includes('\n+') && text.includes('\n-') && /^(?:[+-](?![+-])).+/m.test(text));

  if (!looksDiff) {
    return (
      <pre className={className}>{text.length > 4000 ? `${text.slice(0, 4000)}…` : text}</pre>
    );
  }

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

function ProcessStepRow({ step }: { step: ProcessStep }) {
  const { t } = useI18n();
  if (step.type === 'tool') {
    const input = formatStepInput(step.input);
    return (
      <div className="py-1">
        <div className="font-medium text-secondary">
          ⚙ {step.name} · {step.status}
        </div>
        {input ? (
          <pre className="mt-0.5 max-h-16 overflow-auto whitespace-pre-wrap break-all font-mono text-meta text-muted">
            {input}
          </pre>
        ) : null}
        {step.result ? (
          <DiffAwarePre
            text={step.result}
            className="mt-1 max-h-28 overflow-auto font-mono text-meta leading-relaxed text-secondary"
          />
        ) : null}
      </div>
    );
  }
  if (step.type === 'thinking') {
    return <div className="py-1 italic text-muted">✳ {step.text}</div>;
  }
  if (step.type === 'error') {
    return <div className="py-1 text-danger">{step.message}</div>;
  }
  if (step.type === 'status') {
    return (
      <div className="py-1 text-muted">
        · {step.phase}
        {step.detail ? ` · ${step.detail}` : ''}
      </div>
    );
  }
  return <div className="py-1 text-muted">{stepSummary(step, t)}</div>;
}

function summaryLabel(
  effectivePhase: AgentProcessView['phase'],
  stepCount: number,
  durationMs: number | undefined,
  t: TranslateFn,
): string {
  if (stepCount === 0 && isProcessActivePhase(effectivePhase)) {
    return t('chat.process.summaryGenerating');
  }
  const parts = [processPhaseLabel(effectivePhase, t)];
  if (stepCount > 0) parts.push(t('chat.process.steps', { n: stepCount }));
  if (durationMs != null && durationMs > 0) parts.push(formatDurationMs(durationMs));
  return `▸ ${parts.join(' · ')}`;
}

/**
 * 过程面板（受控 open）：
 * - 进行中 / 失败 / 超时 → 默认展开
 * - 成功 / 取消 → 默认折叠
 * - messageStatus 优先于 process.phase（防止过程机滞后仍停在 running）
 * - 用户点击后记住选择；阶段变化时重新交给自动策略
 */
export function ChatProcessPanel({
  view,
  messageStatus,
  durationMs,
  exitCode,
}: {
  view: AgentProcessView;
  /** 对应气泡消息状态；终态时强制驱动折叠策略 */
  messageStatus?: string;
  durationMs?: number;
  exitCode?: number | null;
}) {
  const { t } = useI18n();
  const timeline = view.steps.filter((s) => s.type !== 'text');

  const effectivePhase: AgentProcessView['phase'] =
    messageStatus && messageStatus !== 'running'
      ? phaseFromMessageStatus(messageStatus)
      : view.phase;

  const autoOpen =
    isProcessActivePhase(effectivePhase) || isProcessErrorPhase(effectivePhase);

  const [userOpen, setUserOpen] = useState<boolean | null>(null);
  const phaseKeyRef = useRef(effectivePhase);

  // 阶段变化（含消息终态到位）时清掉手动覆盖，确保「结束后折叠」生效
  useLayoutEffect(() => {
    if (phaseKeyRef.current !== effectivePhase) {
      phaseKeyRef.current = effectivePhase;
      setUserOpen(null);
    }
  }, [effectivePhase]);

  const open = userOpen ?? autoOpen;
  const hasRunDetails = Boolean(view.command || view.stderr || exitCode != null);

  return (
    <details
      className="mb-2 text-meta text-secondary"
      open={open}
      onToggle={(e) => {
        const next = e.currentTarget.open;
        if (next !== open) {
          setUserOpen(next);
        }
      }}
    >
      <summary className="flex cursor-pointer list-none items-center gap-1.5 py-1 text-muted marker:content-none [&::-webkit-details-marker]:hidden">
        <span className="font-medium text-secondary">
          {summaryLabel(effectivePhase, timeline.length, durationMs, t)}
        </span>
      </summary>
      <div className="space-y-2 pb-1">
        {timeline.length > 0 ? (
          <div className="max-h-48 space-y-0 overflow-y-auto border-l-2 border-border pl-3">
            {timeline.map((step, i) => (
              <ProcessStepRow key={`${step.type}-${i}`} step={step} />
            ))}
          </div>
        ) : isProcessActivePhase(effectivePhase) ? (
          <p className="text-muted">{t('chat.process.waitingLogs')}</p>
        ) : null}
        {hasRunDetails && (
          <details
            className="text-meta"
            onClick={(e) => e.stopPropagation()}
            onToggle={(e) => e.stopPropagation()}
          >
            <summary className="cursor-pointer text-muted">{t('chat.process.runDetails')}</summary>
            <div className="mt-1.5 space-y-2">
              {view.command ? (
                <div>
                  <div className="mb-0.5 text-muted">{t('chat.process.command')}</div>
                  <pre className="max-h-24 overflow-auto whitespace-pre-wrap break-all rounded-btn bg-subtle px-2 py-1.5 font-mono text-meta leading-relaxed text-primary">
                    {view.command}
                  </pre>
                </div>
              ) : null}
              {view.stderr ? (
                <div>
                  <div className="mb-0.5 text-muted">stderr</div>
                  <pre className="max-h-36 overflow-auto whitespace-pre-wrap break-all rounded-btn bg-subtle px-2 py-1.5 font-mono text-meta leading-relaxed text-danger/90">
                    {view.stderr}
                  </pre>
                </div>
              ) : null}
              {exitCode != null ? (
                <div className="text-muted">exit {exitCode}</div>
              ) : null}
            </div>
          </details>
        )}
      </div>
    </details>
  );
}
