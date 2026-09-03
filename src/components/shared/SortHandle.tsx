import type { KeyboardEvent, PointerEvent } from 'react';
import { GripVertical } from 'lucide-react';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Hint } from '@/components/ui/tooltip';
import { cn } from '@/lib/utils';

export function SortHandle({
  id,
  disabled,
  onDragStartId,
  onMoveNeighbor,
  className,
}: {
  id: string;
  disabled?: boolean;
  onDragStartId: (id: string, event: PointerEvent<HTMLSpanElement>) => void;
  onMoveNeighbor?: (id: string, direction: -1 | 1) => void;
  /** Kept for caller compatibility; sort grips intentionally stay neutral. */
  color?: string;
  className?: string;
}) {
  const { t } = useI18n();
  if (disabled) return null;

  const onKeyDown = (event: KeyboardEvent<HTMLSpanElement>) => {
    if (!onMoveNeighbor) return;
    if (event.key === 'ArrowUp') {
      event.preventDefault();
      onMoveNeighbor(id, -1);
    } else if (event.key === 'ArrowDown') {
      event.preventDefault();
      onMoveNeighbor(id, 1);
    }
  };

  return (
    <Hint label={t('common.reorder')}>
      <span
        role="button"
        tabIndex={0}
        aria-label={t('common.reorder')}
        onPointerDown={(event) => onDragStartId(id, event)}
        onKeyDown={onKeyDown}
        className={cn(
          'inline-flex h-7 w-5 shrink-0 cursor-grab touch-none select-none items-center justify-center rounded-btn text-muted transition-colors hover:bg-hover hover:text-primary focus:outline-none focus-visible:ring-1 focus-visible:ring-accent/30',
          'active:cursor-grabbing',
          className,
        )}
      >
        <GripVertical
          size={16}
          strokeWidth={1.6}
          absoluteStrokeWidth
          className="pointer-events-none"
          aria-hidden
        />
      </span>
    </Hint>
  );
}
