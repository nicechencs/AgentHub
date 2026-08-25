import { useCallback, useState, type DragEvent } from 'react';
import { cn } from '@/lib/utils';

export function useSortableDrag(onMove: (fromId: string, toId: string) => void) {
  const [draggingId, setDraggingId] = useState<string | null>(null);
  const [overId, setOverId] = useState<string | null>(null);

  const onDragStartId = useCallback((id: string) => {
    setDraggingId(id);
  }, []);

  const rowProps = useCallback(
    (id: string) => ({
      onDragOver: (event: DragEvent<HTMLDivElement>) => {
        event.preventDefault();
        event.dataTransfer.dropEffect = 'move';
        setOverId(id);
      },
      onDrop: (event: DragEvent<HTMLDivElement>) => {
        event.preventDefault();
        const from = draggingId || event.dataTransfer.getData('text/plain');
        if (from) onMove(from, id);
        setDraggingId(null);
        setOverId(null);
      },
      onDragEnd: () => {
        setDraggingId(null);
        setOverId(null);
      },
      className: cn(
        draggingId === id && 'opacity-50',
        overId === id && draggingId && overId !== draggingId && 'rounded-card ring-1 ring-accent/40',
      ),
    }),
    [draggingId, onMove, overId],
  );

  return { draggingId, overId, onDragStartId, rowProps };
}
