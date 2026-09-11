import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type PointerEvent as ReactPointerEvent,
} from 'react';
import { createIdempotentCleanup } from '@/components/layout/side-split-model';
import {
  clampProcessLogHeight,
  persistProcessLogHeight,
  PROCESS_LOG_HEIGHT_STEP,
  PROCESS_LOG_HEIGHT_STEP_LARGE,
  PROCESS_LOG_SPECS,
  processLogHeightOrDefault,
  readStoredProcessLogHeight,
  type ProcessLogPane,
} from './chat-process-log-model';

const DRAG_THRESHOLD_PX = 4;

export function useProcessLogHeight(pane: ProcessLogPane) {
  const spec = PROCESS_LOG_SPECS[pane];
  const [height, setHeight] = useState<number | null>(() => readStoredProcessLogHeight(pane));
  const resizeCleanupRef = useRef<(() => void) | null>(null);

  const cancelResize = useCallback(() => {
    resizeCleanupRef.current?.();
  }, []);

  useEffect(() => () => cancelResize(), [cancelResize]);

  const persistHeight = useCallback(
    (next: number | null) => {
      if (next == null) {
        setHeight(null);
        persistProcessLogHeight(pane, null);
        return spec.defaultHeight;
      }
      const clamped = clampProcessLogHeight(pane, next);
      setHeight(clamped);
      persistProcessLogHeight(pane, clamped);
      return clamped;
    },
    [pane, spec.defaultHeight],
  );

  const paneHeight = processLogHeightOrDefault(pane, height);

  const onResizeStart = useCallback(
    (e: ReactPointerEvent<HTMLDivElement>) => {
      if (e.button !== 0) return;
      e.preventDefault();
      e.stopPropagation();
      cancelResize();
      const startY = e.clientY;
      const previousHeight = height;
      const startH = paneHeight;
      let moved = false;

      const prevCursor = document.body.style.cursor;
      const prevSelect = document.body.style.userSelect;
      const pointerTarget = e.currentTarget;
      const pointerId = e.pointerId;

      const onMove = (ev: globalThis.PointerEvent): void => {
        if (ev.pointerId !== pointerId) return;
        if (!moved) {
          if (Math.abs(ev.clientY - startY) < DRAG_THRESHOLD_PX) return;
          moved = true;
          document.body.style.cursor = 'row-resize';
          document.body.style.userSelect = 'none';
          setHeight(startH);
        }
        setHeight(clampProcessLogHeight(pane, startH + (ev.clientY - startY)));
      };
      const cleanup = createIdempotentCleanup<[boolean, number?]>(
        (commit: boolean, clientY: number = startY) => {
          if (resizeCleanupRef.current !== cancel) return;
          resizeCleanupRef.current = null;
          if (commit && moved) persistHeight(startH + (clientY - startY));
          else if (moved) setHeight(previousHeight);
          document.body.style.cursor = prevCursor;
          document.body.style.userSelect = prevSelect;
          window.removeEventListener('pointermove', onMove);
          window.removeEventListener('pointerup', onUp);
          window.removeEventListener('pointercancel', onCancel);
          window.removeEventListener('blur', onBlur);
          try {
            pointerTarget.releasePointerCapture(pointerId);
          } catch {
            // The pointer may already have been released by the browser.
          }
        },
      );
      function onUp(ev: globalThis.PointerEvent): void {
        if (ev.pointerId !== pointerId) return;
        cleanup(true, ev.clientY);
      }
      function onCancel(ev: globalThis.PointerEvent): void {
        if (ev.pointerId !== pointerId) return;
        cleanup(false);
      }
      function onBlur() {
        cleanup(false);
      }
      function cancel() {
        cleanup(false);
      }
      resizeCleanupRef.current = cancel;

      window.addEventListener('pointermove', onMove);
      window.addEventListener('pointerup', onUp);
      window.addEventListener('pointercancel', onCancel);
      window.addEventListener('blur', onBlur);
      try {
        pointerTarget.setPointerCapture(pointerId);
      } catch {
        // Keep the window listeners as a compatibility fallback.
      }
    },
    [cancelResize, height, pane, paneHeight, persistHeight],
  );

  const onSeparatorKeyDown = useCallback(
    (e: ReactKeyboardEvent<HTMLDivElement>) => {
      if (e.key === 'ArrowUp' || e.key === 'ArrowDown') {
        e.preventDefault();
        const step = e.shiftKey ? PROCESS_LOG_HEIGHT_STEP_LARGE : PROCESS_LOG_HEIGHT_STEP;
        const delta = e.key === 'ArrowDown' ? step : -step;
        persistHeight(paneHeight + delta);
      } else if (e.key === 'Home') {
        e.preventDefault();
        persistHeight(spec.minHeight);
      } else if (e.key === 'End') {
        e.preventDefault();
        persistHeight(spec.maxHeight);
      }
    },
    [paneHeight, persistHeight, spec.maxHeight, spec.minHeight],
  );

  const resetHeight = useCallback(() => {
    persistHeight(null);
  }, [persistHeight]);

  return {
    paneHeight,
    valuemin: spec.minHeight,
    onResizeStart,
    onSeparatorKeyDown,
    resetHeight,
  };
}
