import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type PointerEvent as ReactPointerEvent,
  type TransitionEvent as ReactTransitionEvent,
} from 'react';
import { usePrefersReducedMotion } from '@/lib/motion';
import type { AgentSession } from '@/lib/types';
import {
  clampProjectPreviewWidth,
  MAIN_WIDTH_MIN,
  persistProjectPreviewWidth,
  PREVIEW_FRAME_PAD_RIGHT,
  PREVIEW_WIDTH_DEFAULT,
  PREVIEW_WIDTH_FLOOR,
  PREVIEW_WIDTH_MIN,
  PREVIEW_WIDTH_STEP,
  PREVIEW_WIDTH_STEP_LARGE,
  readStoredProjectPreviewWidth,
} from './projects-preview-model';

export function createIdempotentCleanup<T extends unknown[]>(cleanup: (...args: T) => void) {
  let completed = false;
  return (...args: T) => {
    if (completed) return;
    completed = true;
    cleanup(...args);
  };
}

export function useProjectPreview() {
  const reduceMotion = usePrefersReducedMotion();
  const [session, setSession] = useState<AgentSession | null>(null);
  const [previewWidth, setPreviewWidth] = useState(readStoredProjectPreviewWidth);
  const [previewShellMounted, setPreviewShellMounted] = useState(false);
  const [previewExpanded, setPreviewExpanded] = useState(false);
  const [previewResizing, setPreviewResizing] = useState(false);
  const splitRef = useRef<HTMLDivElement>(null);
  const previewBodyRef = useRef<HTMLDivElement>(null);
  const epochRef = useRef(0);
  const closingEpochRef = useRef(0);
  const openRafRef = useRef<number | null>(null);
  const resizeCleanupRef = useRef<(() => void) | null>(null);

  const cancelPreviewResize = useCallback(() => {
    resizeCleanupRef.current?.();
  }, []);

  useEffect(() => () => cancelPreviewResize(), [cancelPreviewResize]);

  const cancelOpenRaf = () => {
    if (openRafRef.current == null) return;
    cancelAnimationFrame(openRafRef.current);
    openRafRef.current = null;
  };

  const clampWidth = useCallback((w: number) => {
    const containerW =
      splitRef.current?.getBoundingClientRect().width ??
      (typeof window !== 'undefined' ? window.innerWidth : 1200);
    return clampProjectPreviewWidth(w, containerW);
  }, []);

  const persistWidth = useCallback(
    (w: number) => {
      const next = clampWidth(w);
      setPreviewWidth(next);
      persistProjectPreviewWidth(next);
      return next;
    },
    [clampWidth],
  );

  useEffect(() => {
    if (!previewShellMounted || !previewExpanded) return;
    const el = splitRef.current;
    if (!el || typeof ResizeObserver === 'undefined') {
      const onWin = () => setPreviewWidth((w) => clampWidth(w));
      window.addEventListener('resize', onWin);
      onWin();
      return () => window.removeEventListener('resize', onWin);
    }
    const ro = new ResizeObserver(() => {
      setPreviewWidth((w) => {
        const next = clampWidth(w);
        return next === w ? w : next;
      });
    });
    ro.observe(el);
    setPreviewWidth((w) => clampWidth(w));
    return () => ro.disconnect();
  }, [previewShellMounted, previewExpanded, clampWidth]);

  const open = useCallback(
    (next: AgentSession) => {
      cancelPreviewResize();
      const epoch = ++epochRef.current;
      closingEpochRef.current = 0;
      cancelOpenRaf();
      setSession(next);
      setPreviewShellMounted(true);
      if (reduceMotion) {
        setPreviewExpanded(true);
        return;
      }
      openRafRef.current = requestAnimationFrame(() => {
        openRafRef.current = requestAnimationFrame(() => {
          openRafRef.current = null;
          if (epochRef.current !== epoch) return;
          setPreviewExpanded(true);
        });
      });
    },
    [cancelPreviewResize, reduceMotion],
  );

  const close = useCallback(() => {
    cancelPreviewResize();
    closingEpochRef.current = ++epochRef.current;
    cancelOpenRaf();
    setPreviewExpanded(false);
    if (reduceMotion) {
      setSession(null);
      setPreviewShellMounted(false);
    }
  }, [cancelPreviewResize, reduceMotion]);

  const reset = useCallback(() => {
    cancelPreviewResize();
    epochRef.current += 1;
    closingEpochRef.current = 0;
    cancelOpenRaf();
    setSession(null);
    setPreviewExpanded(false);
    setPreviewShellMounted(false);
  }, [cancelPreviewResize]);

  const onPreviewPaneTransitionEnd = useCallback(
    (e: ReactTransitionEvent<HTMLElement>) => {
      if (e.propertyName !== 'width') return;
      if (previewExpanded) return;
      if (closingEpochRef.current === 0 || closingEpochRef.current !== epochRef.current) return;
      setSession(null);
      setPreviewShellMounted(false);
    },
    [previewExpanded],
  );

  const onPreviewResizeStart = useCallback(
    (e: ReactPointerEvent<HTMLDivElement>) => {
      e.preventDefault();
      cancelPreviewResize();
      const startX = e.clientX;
      const startW = previewWidth;
      const prevCursor = document.body.style.cursor;
      const prevSelect = document.body.style.userSelect;
      const pointerTarget = e.currentTarget;
      const pointerId = e.pointerId;
      document.body.style.cursor = 'col-resize';
      document.body.style.userSelect = 'none';
      setPreviewResizing(true);

      const onMove = (ev: globalThis.PointerEvent): void => {
        if (ev.pointerId !== pointerId) return;
        setPreviewWidth(clampWidth(startW + (startX - ev.clientX)));
      };
      const cleanup = createIdempotentCleanup<[boolean, number?]>(
        (commit: boolean, clientX: number = startX) => {
          if (resizeCleanupRef.current !== cancel) return;
          resizeCleanupRef.current = null;
          if (commit) persistWidth(startW + (startX - clientX));
          document.body.style.cursor = prevCursor;
          document.body.style.userSelect = prevSelect;
          setPreviewResizing(false);
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
    [cancelPreviewResize, previewWidth, clampWidth, persistWidth],
  );

  const onPreviewSeparatorKeyDown = useCallback(
    (e: ReactKeyboardEvent<HTMLDivElement>) => {
      if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') {
        e.preventDefault();
        const step = e.shiftKey ? PREVIEW_WIDTH_STEP_LARGE : PREVIEW_WIDTH_STEP;
        const delta = e.key === 'ArrowLeft' ? step : -step;
        persistWidth(previewWidth + delta);
      } else if (e.key === 'Home') {
        e.preventDefault();
        persistWidth(PREVIEW_WIDTH_MIN);
      } else if (e.key === 'End') {
        e.preventDefault();
        const containerW =
          splitRef.current?.getBoundingClientRect().width ?? window.innerWidth;
        persistWidth(containerW - MAIN_WIDTH_MIN);
      }
    },
    [previewWidth, persistWidth],
  );

  const resetPreviewWidth = useCallback(() => {
    persistWidth(PREVIEW_WIDTH_DEFAULT);
  }, [persistWidth]);

  const previewShellWidth = previewExpanded ? previewWidth + PREVIEW_FRAME_PAD_RIGHT : 0;
  const previewWidthTransition =
    !previewResizing && !reduceMotion ? 'motion-panel-width' : 'transition-none';

  return {
    session,
    sessionId: session?.id ?? null,
    previewWidth,
    previewShellMounted,
    previewExpanded,
    previewResizing,
    previewShellWidth,
    previewWidthTransition,
    splitRef,
    previewBodyRef,
    open,
    close,
    reset,
    onPreviewResizeStart,
    onPreviewSeparatorKeyDown,
    resetPreviewWidth,
    onPreviewPaneTransitionEnd,
    valuemin: PREVIEW_WIDTH_FLOOR,
  };
}
