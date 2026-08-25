import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type PointerEvent as ReactPointerEvent,
  type RefObject,
  type TransitionEvent as ReactTransitionEvent,
} from 'react';
import { usePrefersReducedMotion } from '@/lib/motion';
import {
  clampSideSplitWidth,
  createIdempotentCleanup,
  persistSideSplitWidth,
  readStoredSideSplitWidth,
  SIDE_SPLIT_FRAME_PAD_RIGHT,
  SIDE_SPLIT_MAIN_MIN,
  SIDE_SPLIT_WIDTH_DEFAULT,
  SIDE_SPLIT_WIDTH_FLOOR,
  SIDE_SPLIT_WIDTH_MIN,
  SIDE_SPLIT_WIDTH_STEP,
  SIDE_SPLIT_WIDTH_STEP_LARGE,
} from './side-split-model';

export type SideSplitController<T> = {
  target: T | null;
  paneWidth: number;
  mounted: boolean;
  expanded: boolean;
  resizing: boolean;
  shellWidth: number;
  widthTransition: string;
  splitRef: RefObject<HTMLDivElement>;
  open: (next: T) => void;
  close: () => void;
  reset: () => void;
  onResizeStart: (e: ReactPointerEvent<HTMLDivElement>) => void;
  onSeparatorKeyDown: (e: ReactKeyboardEvent<HTMLDivElement>) => void;
  resetWidth: () => void;
  onPaneTransitionEnd: (e: ReactTransitionEvent<HTMLElement>) => void;
  valuemin: number;
};

export function useSideSplit<T>(options: {
  storageKey: string;
  defaultWidth?: number;
}): SideSplitController<T> {
  const reduceMotion = usePrefersReducedMotion();
  const storageKey = options.storageKey;
  const defaultWidth = options.defaultWidth ?? SIDE_SPLIT_WIDTH_DEFAULT;
  const [target, setTarget] = useState<T | null>(null);
  const [paneWidth, setPaneWidth] = useState(() =>
    readStoredSideSplitWidth(storageKey, defaultWidth),
  );
  const [mounted, setMounted] = useState(false);
  const [expanded, setExpanded] = useState(false);
  const [resizing, setResizing] = useState(false);
  const splitRef = useRef<HTMLDivElement>(null);
  const epochRef = useRef(0);
  const closingEpochRef = useRef(0);
  const openRafRef = useRef<number | null>(null);
  const resizeCleanupRef = useRef<(() => void) | null>(null);
  const expandedRef = useRef(false);
  expandedRef.current = expanded;

  const cancelResize = useCallback(() => {
    resizeCleanupRef.current?.();
  }, []);

  useEffect(() => () => cancelResize(), [cancelResize]);

  const cancelOpenRaf = () => {
    if (openRafRef.current == null) return;
    cancelAnimationFrame(openRafRef.current);
    openRafRef.current = null;
  };

  const clampWidth = useCallback(
    (w: number) => {
      const containerW =
        splitRef.current?.getBoundingClientRect().width ??
        (typeof window !== 'undefined' ? window.innerWidth : 1200);
      return clampSideSplitWidth(w, containerW);
    },
    [],
  );

  const persistWidth = useCallback(
    (w: number) => {
      const next = clampWidth(w);
      setPaneWidth(next);
      persistSideSplitWidth(storageKey, next);
      return next;
    },
    [clampWidth, storageKey],
  );

  useEffect(() => {
    if (!mounted || !expanded) return;
    const el = splitRef.current;
    if (!el || typeof ResizeObserver === 'undefined') {
      const onWin = () => setPaneWidth((w) => clampWidth(w));
      window.addEventListener('resize', onWin);
      onWin();
      return () => window.removeEventListener('resize', onWin);
    }
    const ro = new ResizeObserver(() => {
      setPaneWidth((w) => {
        const next = clampWidth(w);
        return next === w ? w : next;
      });
    });
    ro.observe(el);
    setPaneWidth((w) => clampWidth(w));
    return () => ro.disconnect();
  }, [mounted, expanded, clampWidth]);

  const open = useCallback(
    (next: T) => {
      cancelResize();
      const epoch = ++epochRef.current;
      closingEpochRef.current = 0;
      cancelOpenRaf();
      setTarget(next);
      setMounted(true);
      if (reduceMotion || expandedRef.current) {
        setExpanded(true);
        return;
      }
      openRafRef.current = requestAnimationFrame(() => {
        openRafRef.current = requestAnimationFrame(() => {
          openRafRef.current = null;
          if (epochRef.current !== epoch) return;
          setExpanded(true);
        });
      });
    },
    [cancelResize, reduceMotion],
  );

  const close = useCallback(() => {
    cancelResize();
    closingEpochRef.current = ++epochRef.current;
    cancelOpenRaf();
    setExpanded(false);
    if (reduceMotion) {
      setTarget(null);
      setMounted(false);
    }
  }, [cancelResize, reduceMotion]);

  const reset = useCallback(() => {
    cancelResize();
    epochRef.current += 1;
    closingEpochRef.current = 0;
    cancelOpenRaf();
    setTarget(null);
    setExpanded(false);
    setMounted(false);
  }, [cancelResize]);

  const onPaneTransitionEnd = useCallback(
    (e: ReactTransitionEvent<HTMLElement>) => {
      if (e.propertyName !== 'width') return;
      if (expanded) return;
      if (closingEpochRef.current === 0 || closingEpochRef.current !== epochRef.current) return;
      setTarget(null);
      setMounted(false);
    },
    [expanded],
  );

  const onResizeStart = useCallback(
    (e: ReactPointerEvent<HTMLDivElement>) => {
      e.preventDefault();
      cancelResize();
      const startX = e.clientX;
      const startW = paneWidth;
      const prevCursor = document.body.style.cursor;
      const prevSelect = document.body.style.userSelect;
      const pointerTarget = e.currentTarget;
      const pointerId = e.pointerId;
      document.body.style.cursor = 'col-resize';
      document.body.style.userSelect = 'none';
      setResizing(true);

      const onMove = (ev: globalThis.PointerEvent): void => {
        if (ev.pointerId !== pointerId) return;
        setPaneWidth(clampWidth(startW + (startX - ev.clientX)));
      };
      const cleanup = createIdempotentCleanup<[boolean, number?]>(
        (commit: boolean, clientX: number = startX) => {
          if (resizeCleanupRef.current !== cancel) return;
          resizeCleanupRef.current = null;
          if (commit) persistWidth(startW + (startX - clientX));
          document.body.style.cursor = prevCursor;
          document.body.style.userSelect = prevSelect;
          setResizing(false);
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
        cleanup(true, ev.clientX);
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
    [cancelResize, paneWidth, clampWidth, persistWidth],
  );

  const onSeparatorKeyDown = useCallback(
    (e: ReactKeyboardEvent<HTMLDivElement>) => {
      if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') {
        e.preventDefault();
        const step = e.shiftKey ? SIDE_SPLIT_WIDTH_STEP_LARGE : SIDE_SPLIT_WIDTH_STEP;
        const delta = e.key === 'ArrowLeft' ? step : -step;
        persistWidth(paneWidth + delta);
      } else if (e.key === 'Home') {
        e.preventDefault();
        persistWidth(SIDE_SPLIT_WIDTH_MIN);
      } else if (e.key === 'End') {
        e.preventDefault();
        const containerW =
          splitRef.current?.getBoundingClientRect().width ?? window.innerWidth;
        persistWidth(containerW - SIDE_SPLIT_MAIN_MIN);
      }
    },
    [paneWidth, persistWidth],
  );

  const resetWidth = useCallback(() => {
    persistWidth(defaultWidth);
  }, [persistWidth, defaultWidth]);

  const shellWidth = expanded ? paneWidth + SIDE_SPLIT_FRAME_PAD_RIGHT : 0;
  const widthTransition =
    !resizing && !reduceMotion ? 'motion-panel-width' : 'transition-none';

  return {
    target,
    paneWidth,
    mounted,
    expanded,
    resizing,
    shellWidth,
    widthTransition,
    splitRef,
    open,
    close,
    reset,
    onResizeStart,
    onSeparatorKeyDown,
    resetWidth,
    onPaneTransitionEnd,
    valuemin: SIDE_SPLIT_WIDTH_FLOOR,
  };
}
