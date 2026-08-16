import { useLayoutEffect, useRef, useState } from 'react';
import { Terminal } from 'lucide-react';
import { Tip } from '@/components/ui/tooltip';
import {
  phaseFromMessageStatus,
  processPhaseLabel,
  stepSummary,
  type AgentProcessView,
} from '@/lib/chat-process';
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
        <div className="text-muted">…已截断</div>
      ) : null}
    </pre>
  );
}

function ProcessStepRow({ step }: { step: ProcessStep }) {
  if (step.type === 'tool') {
    const input = formatStepInput(step.input);
    return (
      <div className="rounded-btn border border-border/80 bg-panel px-2 py-1.5">
        <div className="flex items-center gap-1.5 font-medium text-secondary">
          <span className="rounded-btn bg-subtle px-1 py-0.5 text-meta uppercase tracking-wide text-muted">
            tool
          </span>
          <span>{step.name}</span>
          <span className="text-muted">· {step.status}</span>
        </div>
        {input ? (
          <pre className="mt-1 max-h-16 overflow-auto whitespace-pre-wrap break-all font-mono text-meta text-muted">
            {input}
          </pre>
        ) : null}
        {step.result ? (
          <DiffAwarePre
            text={step.result}
            className="mt-1 max-h-28 overflow-auto rounded-btn bg-subtle/50 px-1.5 py-1 font-mono text-meta leading-relaxed text-secondary"
          />
        ) : null}
      </div>
    );
  }
  if (step.type === 'thinking') {
    return (
      <div className="rounded-btn border border-dashed border-border/80 px-2 py-1.5 text-muted">
        <span className="mr-1.5 text-meta uppercase tracking-wide">thinking</span>
        <span className="whitespace-pre-wrap">{step.text}</span>
      </div>
    );
  }
  if (step.type === 'error') {
    return <div className="text-danger">{step.message}</div>;
  }
  return (
    <div className="text-muted">
      <span className="mr-1 font-medium text-secondary">{step.type}</span>
      {stepSummary(step)}
    </div>
  );
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
  const timeline = view.steps.filter((s) => s.type !== 'text');
  const toolCount = timeline.filter((s) => s.type === 'tool').length;
  const thinkingCount = timeline.filter((s) => s.type === 'thinking').length;

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

  return (
    <details
      className="mb-2 rounded-card border border-border bg-subtle/60 text-xs text-secondary"
      open={open}
      onToggle={(e) => {
        const next = e.currentTarget.open;
        // 与受控 open 对齐：只在用户意图与当前受控值不同时写入
        if (next !== open) {
          setUserOpen(next);
        }
      }}
    >
      <summary className="flex cursor-pointer list-none items-center gap-1.5 px-2.5 py-1.5 text-muted marker:content-none [&::-webkit-details-marker]:hidden">
        <Terminal className="h-3 w-3 shrink-0 opacity-70" />
        <span className="font-medium text-secondary">过程</span>
        <span className="text-muted">·</span>
        <span>{processPhaseLabel(effectivePhase)}</span>
        {timeline.length > 0 ? (
          <span className="text-muted">· {timeline.length} 步</span>
        ) : null}
        {durationMs != null && durationMs > 0 ? (
          <span className="text-muted">· {formatDurationMs(durationMs)}</span>
        ) : null}
        {view.command ? (
          <Tip
            className="ml-auto max-w-[45%] truncate font-mono text-meta text-muted"
            label={view.command}
          >
            {view.command}
          </Tip>
        ) : null}
      </summary>
      <div className="space-y-2 border-t border-border px-2.5 py-2">
        <div className="flex flex-wrap items-center gap-x-2 gap-y-0.5">
          <span className="text-muted">状态</span>
          <span className="font-medium text-secondary">
            {processPhaseLabel(effectivePhase)}
          </span>
          {durationMs != null && durationMs > 0 ? (
            <>
              <span className="text-muted">·</span>
              <span className="text-muted">耗时 {formatDurationMs(durationMs)}</span>
            </>
          ) : null}
          {exitCode != null ? (
            <>
              <span className="text-muted">·</span>
              <span className="text-muted">exit {exitCode}</span>
            </>
          ) : null}
          {toolCount > 0 || thinkingCount > 0 ? (
            <>
              <span className="text-muted">·</span>
              <span className="text-muted">
                {[toolCount > 0 ? `工具 ${toolCount}` : null, thinkingCount > 0 ? `思考 ${thinkingCount}` : null]
                  .filter(Boolean)
                  .join(' · ')}
              </span>
            </>
          ) : null}
        </div>
        {view.command ? (
          <div>
            <div className="mb-0.5 text-muted">命令</div>
            <pre className="max-h-24 overflow-auto whitespace-pre-wrap break-all rounded-btn bg-panel px-2 py-1.5 font-mono text-meta leading-relaxed text-primary">
              {view.command}
            </pre>
          </div>
        ) : null}
        {timeline.length > 0 ? (
          <div>
            <div className="mb-1 text-muted">步骤</div>
            <div className="max-h-48 space-y-1.5 overflow-y-auto">
              {timeline.map((step, i) => (
                <ProcessStepRow key={`${step.type}-${i}`} step={step} />
              ))}
            </div>
          </div>
        ) : null}
        {view.stderr ? (
          <div>
            <div className="mb-0.5 text-muted">stderr</div>
            <pre className="max-h-36 overflow-auto whitespace-pre-wrap break-all rounded-btn bg-panel px-2 py-1.5 font-mono text-meta leading-relaxed text-danger/90">
              {view.stderr}
            </pre>
          </div>
        ) : !timeline.length && isProcessActivePhase(effectivePhase) ? (
          <p className="text-muted">等待 CLI 输出过程日志…</p>
        ) : null}
      </div>
    </details>
  );
}
