import type { KeyboardEvent as ReactKeyboardEvent, PointerEvent as ReactPointerEvent, ReactNode } from 'react';
import { cn } from '@/lib/utils';
import type { ProcessLogPane } from './chat-process-log-model';
import { useProcessLogHeight } from './use-process-log-height';

/** Shared frame for code, JSON, logs, and other scrollable snippets. */
export const SNIPPET_SURFACE =
  'overflow-auto [overflow-anchor:none] whitespace-pre-wrap break-words rounded-btn border border-border bg-subtle font-mono text-meta leading-relaxed text-primary';

export function HeightSeparator({
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
  onResizeStart: (e: ReactPointerEvent<HTMLDivElement>) => void;
  onSeparatorKeyDown: (e: ReactKeyboardEvent<HTMLDivElement>) => void;
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

export function ResizableRegion({
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
    <div className={cn(className)}>
      {children(height.paneHeight)}
      <HeightSeparator label={label} {...height} />
    </div>
  );
}
