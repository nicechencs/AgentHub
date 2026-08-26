import type { KeyboardEvent, PointerEvent } from 'react';
import { GripVertical } from 'lucide-react';
import { useI18n } from '@/components/shared/LanguageProvider';
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
    <span
      role="button"
      tabIndex={0}
      aria-label={t('common.reorder')}
      title={t('common.reorder')}
      onPointerDown={(event) => onDragStartId(id, event)}
      onKeyDown={onKeyDown}
      className={cn(
        'inline-flex h-7 w-5 shrink-0 cursor-grab touch-none select-none items-center justify-center rounded-btn text-muted',
        'hover:bg-hover hover:text-primary active:cursor-grabbing',
        className,
      )}
    >
      <GripVertical className="pointer-events-none h-4 w-4" aria-hidden />
    </span>
  );
}
