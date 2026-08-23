import { Hint } from '@/components/ui/tooltip';
import { cn } from '@/lib/utils';

const SIZE = {
  sm: 'h-1.5 w-1.5',
  md: 'h-2 w-2',
} as const;

const TONE = {
  success: 'bg-success',
  warning: 'bg-warning',
  danger: 'bg-danger',
  info: 'bg-info',
  muted: 'bg-muted',
} as const;

const RING = {
  panel: 'ring-2 ring-panel',
  warning: 'ring-2 ring-warning',
} as const;

/**
 * Tiny semantic pin (update notice / effective connection / missing runtime).
 * Prefer this over hand-rolled `h-1.5 w-1.5 rounded-full bg-*` spans.
 */
export function StatusPin({
  tone,
  size = 'sm',
  ring,
  label,
  className,
  corner,
}: {
  tone: keyof typeof TONE;
  size?: keyof typeof SIZE;
  ring?: keyof typeof RING | false;
  /** When set, wraps with Hint. */
  label?: string | null;
  className?: string;
  /** Absolute corner placement (e.g. collapsed sidebar icon badge). */
  corner?: boolean;
}) {
  const pin = (
    <span
      className={cn(
        'inline-block shrink-0 rounded-full',
        SIZE[size],
        TONE[tone],
        ring && RING[ring],
        corner && 'absolute -right-0.5 -top-0.5',
        className,
      )}
      aria-hidden={label ? undefined : true}
    />
  );

  if (!label) return pin;
  return <Hint label={label}>{pin}</Hint>;
}
