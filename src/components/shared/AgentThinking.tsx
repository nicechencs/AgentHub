import { useEffect, useState } from 'react';
import { cn } from '@/lib/utils';

/** Three bouncing dots + elapsed time while a reply has no text yet. */
export function AgentThinking({
  label,
  className,
  showTimer = true,
}: {
  label: string;
  className?: string;
  showTimer?: boolean;
}) {
  const [elapsed, setElapsed] = useState(0);

  useEffect(() => {
    const started = Date.now();
    const id = window.setInterval(() => {
      setElapsed(Math.max(0, Math.floor((Date.now() - started) / 1000)));
    }, 1000);
    return () => window.clearInterval(id);
  }, []);

  return (
    <span
      role="status"
      className={cn('inline-flex items-center gap-2 text-body text-secondary', className)}
    >
      <span className="agent-thinking-dots shrink-0" aria-hidden>
        <span />
        <span />
        <span />
      </span>
      <span>{label}</span>
      {showTimer ? (
        <span className="font-mono text-meta tabular-nums text-muted">{elapsed}s</span>
      ) : null}
    </span>
  );
}
