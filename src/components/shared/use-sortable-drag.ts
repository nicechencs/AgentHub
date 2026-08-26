import { useCallback, useEffect, useRef, useState, type PointerEvent as ReactPointerEvent } from 'react';
import { cn } from '@/lib/utils';
import {
  grabOffset,
  previewOrigin,
  previewTransform,
  SORTABLE_ID_ATTR,
  SORTABLE_PREVIEW_ATTR,
  SORTABLE_PREVIEW_SCALE,
  SORTABLE_PREVIEW_Z,
} from './sortable-drag-model';

export { SORTABLE_ID_ATTR };

export function readSortableIdFromPoint(clientX: number, clientY: number): string | null {
  const node = document.elementFromPoint(clientX, clientY);
  if (!node) return null;
  return node.closest(`[${SORTABLE_ID_ATTR}]`)?.getAttribute(SORTABLE_ID_ATTR) ?? null;
}

function reducedMotionScale(): number {
  try {
    return window.matchMedia('(prefers-reduced-motion: reduce)').matches ? 1 : SORTABLE_PREVIEW_SCALE;
  } catch {
    return SORTABLE_PREVIEW_SCALE;
  }
}

function mountDragPreview(row: HTMLElement, clientX: number, clientY: number): {
  move: (clientX: number, clientY: number) => void;
  remove: () => void;
} {
  const rect = row.getBoundingClientRect();
  const offset = grabOffset(clientX, clientY, rect.left, rect.top);
  const scale = reducedMotionScale();
  const ghost = row.cloneNode(true) as HTMLElement;
  ghost.removeAttribute(SORTABLE_ID_ATTR);
  ghost.setAttribute(SORTABLE_PREVIEW_ATTR, 'true');
  ghost.setAttribute('aria-hidden', 'true');
  ghost.querySelectorAll('button, a, input, [tabindex]').forEach((el) => {
    el.setAttribute('tabindex', '-1');
  });
  ghost.style.position = 'fixed';
  ghost.style.left = '0';
  ghost.style.top = '0';
  ghost.style.width = `${rect.width}px`;
  ghost.style.margin = '0';
  ghost.style.zIndex = SORTABLE_PREVIEW_Z;
  ghost.style.pointerEvents = 'none';
  ghost.style.transformOrigin = 'top left';
  ghost.style.boxShadow = 'var(--shadow-md)';
  ghost.style.opacity = '0.96';
  ghost.style.willChange = 'transform';
  const place = (x: number, y: number) => {
    const origin = previewOrigin(x, y, offset.x, offset.y);
    ghost.style.transform = previewTransform(origin.x, origin.y, scale);
  };
  place(clientX, clientY);
  document.body.appendChild(ghost);
  return {
    move: place,
    remove: () => {
      ghost.remove();
    },
  };
}

function sortableRowFromHandle(start: EventTarget | null): HTMLElement | null {
  if (!(start instanceof Element)) return null;
  const row = start.closest(`[${SORTABLE_ID_ATTR}]`);
  return row instanceof HTMLElement ? row : null;
}

export function useSortableDrag(onMove: (fromId: string, toId: string) => void) {
  const [draggingId, setDraggingId] = useState<string | null>(null);
  const [overId, setOverId] = useState<string | null>(null);
  const draggingIdRef = useRef<string | null>(null);
  const overIdRef = useRef<string | null>(null);
  const onMoveRef = useRef(onMove);
  onMoveRef.current = onMove;
  const stopDragRef = useRef<(() => void) | null>(null);

  const stopDrag = useCallback(() => {
    stopDragRef.current?.();
  }, []);

  useEffect(() => () => stopDrag(), [stopDrag]);

  const onDragStartId = useCallback((id: string, event: ReactPointerEvent<HTMLElement>) => {
    if (event.button !== 0) return;
    event.preventDefault();
    event.stopPropagation();
    stopDrag();

    draggingIdRef.current = id;
    overIdRef.current = id;
    setDraggingId(id);
    setOverId(id);

    const prevCursor = document.body.style.cursor;
    const prevSelect = document.body.style.userSelect;
    document.body.style.cursor = 'grabbing';
    document.body.style.userSelect = 'none';
    const pointerId = event.pointerId;
    const row = sortableRowFromHandle(event.currentTarget);
    const preview = row ? mountDragPreview(row, event.clientX, event.clientY) : null;

    const onMovePtr = (ev: PointerEvent): void => {
      if (ev.pointerId !== pointerId) return;
      preview?.move(ev.clientX, ev.clientY);
      const next = readSortableIdFromPoint(ev.clientX, ev.clientY);
      if (!next || next === overIdRef.current) return;
      overIdRef.current = next;
      setOverId(next);
    };

    let finished = false;
    const finish = (commit: boolean) => {
      if (finished) return;
      finished = true;
      if (stopDragRef.current === cancel) stopDragRef.current = null;
      const from = draggingIdRef.current;
      const to = overIdRef.current;
      draggingIdRef.current = null;
      overIdRef.current = null;
      setDraggingId(null);
      setOverId(null);
      preview?.remove();
      document.body.style.cursor = prevCursor;
      document.body.style.userSelect = prevSelect;
      window.removeEventListener('pointermove', onMovePtr);
      window.removeEventListener('pointerup', onUp);
      window.removeEventListener('pointercancel', onCancel);
      window.removeEventListener('blur', onBlur);
      if (commit && from && to && from !== to) onMoveRef.current(from, to);
    };

    function onUp(ev: PointerEvent): void {
      if (ev.pointerId !== pointerId) return;
      finish(true);
    }
    function onCancel(ev: PointerEvent): void {
      if (ev.pointerId !== pointerId) return;
      finish(false);
    }
    function onBlur() {
      finish(false);
    }
    function cancel() {
      finish(false);
    }
    stopDragRef.current = cancel;

    window.addEventListener('pointermove', onMovePtr);
    window.addEventListener('pointerup', onUp);
    window.addEventListener('pointercancel', onCancel);
    window.addEventListener('blur', onBlur);
  }, [stopDrag]);

  const rowProps = useCallback(
    (id: string) => ({
      [SORTABLE_ID_ATTR]: id,
      className: cn(
        draggingId === id && 'opacity-40',
        overId === id && draggingId && overId !== draggingId && 'rounded-card ring-1 ring-accent/40',
      ),
    }),
    [draggingId, overId],
  );

  return { draggingId, overId, onDragStartId, rowProps };
}
