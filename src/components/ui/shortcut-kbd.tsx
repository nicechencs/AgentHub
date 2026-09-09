import type { ReactNode } from 'react';
import { CornerDownLeft } from 'lucide-react';
import { cn } from '@/lib/utils';

/** Same kbd chip as the Chat shortcut overlay. */
export function ShortcutKbd({
  children,
  onAccent = false,
  className,
}: {
  children: ReactNode;
  onAccent?: boolean;
  className?: string;
}) {
  return (
    <kbd
      className={cn(
        'inline-flex h-5 min-w-5 items-center justify-center rounded-btn border px-1.5 text-meta leading-none',
        onAccent ? 'border-white/35 bg-white/15 text-white' : 'border-border bg-subtle text-muted',
        className,
      )}
      aria-hidden
    >
      {children}
    </kbd>
  );
}

/** Enter / Return mark — icon only, no “Enter” letters. */
export function EnterKeyMark({ onAccent = false }: { onAccent?: boolean }) {
  return (
    <ShortcutKbd onAccent={onAccent}>
      <CornerDownLeft className="h-3 w-3" strokeWidth={2.25} />
    </ShortcutKbd>
  );
}
