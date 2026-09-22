import { ChevronRight } from 'lucide-react';
import { Tip } from '@/components/ui/tooltip';
import { cn } from '@/lib/utils';
import { ICON } from '@/styles/tokens';

/**
 * Shared disclosure cue for plan / process / changed-file rows.
 * Stronger than a low-contrast ▸ so the row reads as expandable.
 */
export function ChatExpandAffordance({
  expanded,
  label,
}: {
  expanded: boolean;
  label?: string;
}) {
  const mark = (
    <span
      className={cn(
        'inline-flex h-4 w-4 shrink-0 items-center justify-center rounded-btn text-secondary',
        'transition-colors group-hover:text-primary group-hover:bg-hover',
        expanded && 'text-primary',
      )}
      data-help="chat-expand-affordance"
      aria-hidden
    >
      <ChevronRight
        className={cn(ICON.chrome.className, 'transition-transform', expanded && 'rotate-90')}
        strokeWidth={2.25}
      />
    </span>
  );
  return (
    <>
      {label ? <Tip label={label}>{mark}</Tip> : mark}
      {label ? <span className="sr-only">{label}</span> : null}
    </>
  );
}
