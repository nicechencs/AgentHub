import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type PointerEvent as ReactPointerEvent,
} from 'react';
import {
  clampComposerPaneHeight,
  COMPOSER_PANE_MIN,
  COMPOSER_PANE_STEP,
  COMPOSER_PANE_STEP_LARGE,
  composerPaneMaxHeight,
  persistComposerPaneHeight,
  readStoredComposerPaneHeight,
} from './chat-split-model';

export function createIdempotentCleanup<T extends unknown[]>(cleanup: (...args: T) => void) {
  let completed = false;
  return (...args: T) => {
    if (completed) return;
    completed = true;
    cleanup(...args);
  };
}

const DRAG_THRESHOLD_PX = 4;

export function useChatComposerSplit() {
  const [composerHeight, setComposerHeight] = useState<number | null>(readStoredComposerPaneHeight);
  const [stageHeight, setStageHeight] = useState(0);
  const splitRef = useRef<HTMLDivElement | null>(null);
  const [splitNode, setSplitNode] = useState<HTMLDivElement | null>(null);
  const composerPaneRef = useRef<HTMLDivElement>(null);
  const resizeCleanupRef = useRef<(() => void) | null>(null);

  const assignSplitRef = useCallback((node: HTMLDivElement | null) => {
    splitRef.current = node;
    setSplitNode((prev) => (prev === node ? prev : node));
  }, []);

  const cancelResize = useCallback(() => {
    resizeCleanupRef.current?.();
  }, []);

  useEffect(() => () => cancelResize(), [cancelResize]);

  const measureStage = useCallback(() => {
    return splitRef.current?.getBoundingClientRect().height ?? 0;
  }, []);

  const clampHeight = useCallback(
    (h: number, stage = stageHeight || measureStage()) => clampComposerPaneHeight(h, stage),
    [stageHeight, measureStage],
  );

  useLayoutEffect(() => {
    const el = splitNode;
    if (!el) return;
    const apply = () => {
      const next = el.getBoundingClientRect().height;
      setStageHeight((prev) => (prev === next ? prev : next));
    };
    apply();
    if (typeof ResizeObserver === 'undefined') {
      window.addEventListener('resize', apply);
      return () => window.removeEventListener('resize', apply);
    }
    const ro = new ResizeObserver(apply);
    ro.observe(el);
    return () => ro.disconnect();
  }, [splitNode]);

  const persistHeight = useCallback(
    (h: number | null) => {
      if (h == null) {
        setComposerHeight(null);
        persistComposerPaneHeight(null);
        return null;
      }
      const next = clampHeight(h);
      setComposerHeight(next);
      persistComposerPaneHeight(next);
      return next;
    },
    [clampHeight],
  );

  const paneHeight =
    composerHeight == null ? null : clampComposerPaneHeight(composerHeight, stageHeight);

  const onResizeStart = useCallback(
    (e: ReactPointerEvent<HTMLDivElement>) => {
      if (e.button !== 0) return;
      e.preventDefault();
      cancelResize();
      const startY = e.clientY;
      const previousHeight = composerHeight;
      const measured =
        composerPaneRef.current?.getBoundingClientRect().height ?? COMPOSER_PANE_MIN;
      const startH = clampHeight(composerHeight ?? measured);
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
          setComposerHeight(startH);
        }
        setComposerHeight(clampHeight(startH - (ev.clientY - startY)));
      };
      const cleanup = createIdempotentCleanup<[boolean, number?]>(
        (commit: boolean, clientY: number = startY) => {
          if (resizeCleanupRef.current !== cancel) return;
          resizeCleanupRef.current = null;
          if (commit && moved) persistHeight(startH - (clientY - startY));
          else if (moved) setComposerHeight(previousHeight);
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
    [cancelResize, composerHeight, clampHeight, persistHeight],
  );

  const onSeparatorKeyDown = useCallback(
    (e: ReactKeyboardEvent<HTMLDivElement>) => {
      const current =
        paneHeight ??
        composerPaneRef.current?.getBoundingClientRect().height ??
        COMPOSER_PANE_MIN;
      if (e.key === 'ArrowUp' || e.key === 'ArrowDown') {
        e.preventDefault();
        const step = e.shiftKey ? COMPOSER_PANE_STEP_LARGE : COMPOSER_PANE_STEP;
        const delta = e.key === 'ArrowUp' ? step : -step;
        persistHeight(current + delta);
      } else if (e.key === 'Home') {
        e.preventDefault();
        persistHeight(COMPOSER_PANE_MIN);
      } else if (e.key === 'End') {
        e.preventDefault();
        persistHeight(composerPaneMaxHeight(stageHeight || measureStage()));
      }
    },
    [paneHeight, persistHeight, stageHeight, measureStage],
  );

  const resetHeight = useCallback(() => {
    persistHeight(null);
  }, [persistHeight]);

  return {
    splitRef: assignSplitRef,
    composerPaneRef,
    paneHeight,
    onResizeStart,
    onSeparatorKeyDown,
    resetHeight,
    valuemin: COMPOSER_PANE_MIN,
  };
}
