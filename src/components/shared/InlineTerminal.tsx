import * as React from 'react';
import { CheckCircle2, Loader2, XCircle } from 'lucide-react';
import { cn } from '@/lib/utils';

export type TerminalStatus = 'running' | 'done' | 'failed';

function formatElapsed(seconds: number): string {
  if (seconds < 60) return `${seconds}s`;
  const m = Math.floor(seconds / 60);
  const s = seconds % 60;
  return `${m}m ${s.toString().padStart(2, '0')}s`;
}

/** 安装/升级的流式输出面板(docs/ui-design.md §5 InlineTerminal) */
export function InlineTerminal({
  lines,
  status,
  className,
  /** Elapsed seconds while running (optional live timer). */
  elapsedSec,
}: {
  lines: string[];
  status: TerminalStatus;
  className?: string;
  elapsedSec?: number;
}) {
  const endRef = React.useRef<HTMLDivElement>(null);
  React.useEffect(() => {
    endRef.current?.scrollIntoView({ block: 'nearest' });
  }, [lines.length, status]);

  return (
    <div
      className={cn(
        'max-h-56 overflow-y-auto rounded-card border border-border bg-canvas p-3 font-mono text-xs leading-5',
        className,
      )}
    >
      {lines.map((line, i) => (
        <div key={i} className="whitespace-pre-wrap text-secondary">
          {line}
        </div>
      ))}
      {status === 'running' && (
        <div className="mt-1 flex items-center gap-1.5 text-muted">
          <Loader2 className="h-3.5 w-3.5 shrink-0 animate-spin" />
          <span className="animate-pulse">
            {typeof elapsedSec === 'number' && elapsedSec > 0
              ? `进行中 · 已等待 ${formatElapsed(elapsedSec)}（下载安装可能需数分钟）`
              : '进行中…'}
          </span>
        </div>
      )}
      {status === 'done' && (
        <div className="mt-1 flex items-center gap-1 text-success">
          <CheckCircle2 className="h-3.5 w-3.5" /> 完成
        </div>
      )}
      {status === 'failed' && (
        <div className="mt-1 flex items-center gap-1 text-danger">
          <XCircle className="h-3.5 w-3.5" /> 失败,请尝试手动执行上方命令
        </div>
      )}
      <div ref={endRef} />
    </div>
  );
}
