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
  clampNavWidth,
  navWidthBounds,
  persistSidebarWidth,
  PRIMARY_NAV_WIDTH,
  readStoredSidebarWidth,
  type NavWidthPolicy,
} from '@/components/layout/sidebar-width-model';
import { usePrefersReducedMotion } from '@/lib/motion';
import { StorageKey } from '@/lib/storage-key';

function viewportWidth(): number {
  return typeof window === 'undefined' ? 0 : window.innerWidth;
}

export function useNavWidth(options: {
  collapsed: boolean;
  storageKey: string;
  policy: NavWidthPolicy;
}) {
  const { collapsed, storageKey, policy } = options;
  const reduceMotion = usePrefersReducedMotion();
  const [rememberedWidth, setRememberedWidth] = useState(() =>
    readStoredSidebarWidth(storageKey, policy.defaultWidth),
  );
  const [stageWidth, setStageWidth] = useState(viewportWidth);
  const [resizing, setResizing] = useState(false);
  const resizeCleanupRef = useRef<(() => void) | null>(null);

  const cancelResize = useCallback(() => {
    resizeCleanupRef.current?.();
  }, []);

  useEffect(() => () => cancelResize(), [cancelResize]);

  useEffect(() => {
    const apply = () => setStageWidth(viewportWidth());
    apply();
    window.addEventListener('resize', apply);
    return () => window.removeEventListener('resize', apply);
  }, []);

  const paneWidth = clampNavWidth(rememberedWidth, stageWidth, policy);
  const { min, max } = navWidthBounds(stageWidth, policy);

  const persistWidth = useCallback(
    (w: number) => {
      const next = clampNavWidth(w, viewportWidth(), policy);
      setRememberedWidth(next);
      persistSidebarWidth(storageKey, next);
      return next;
    },
    [policy, storageKey],
  );

  const onResizeStart = useCallback(
    (e: ReactPointerEvent<HTMLDivElement>) => {
      e.preventDefault();
      e.stopPropagation();
      cancelResize();
      const startX = e.clientX;
      const previousWidth = rememberedWidth;
      const startW = paneWidth;
      const stage = viewportWidth();
      const prevCursor = document.body.style.cursor;
      const prevSelect = document.body.style.userSelect;
      const pointerTarget = e.currentTarget;
      const pointerId = e.pointerId;
      document.body.style.cursor = 'col-resize';
      document.body.style.userSelect = 'none';
      setResizing(true);

      const onMove = (ev: globalThis.PointerEvent): void => {
        if (ev.pointerId !== pointerId) return;
        setRememberedWidth(clampNavWidth(startW + (ev.clientX - startX), stage, policy));
      };
      const cleanup = createIdempotentCleanup<[boolean, number?]>(
        (commit: boolean, clientX: number = startX) => {
          if (resizeCleanupRef.current !== cancel) return;
          resizeCleanupRef.current = null;
          if (commit && clientX !== startX) persistWidth(startW + (clientX - startX));
          else setRememberedWidth(previousWidth);
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
    [cancelResize, paneWidth, persistWidth, policy, rememberedWidth],
  );

  const onSeparatorKeyDown = useCallback(
    (e: ReactKeyboardEvent<HTMLDivElement>) => {
      if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') {
        e.preventDefault();
        const step = e.shiftKey ? policy.stepLarge : policy.step;
        const delta = e.key === 'ArrowRight' ? step : -step;
        persistWidth(paneWidth + delta);
      } else if (e.key === 'Home') {
        e.preventDefault();
        persistWidth(min);
      } else if (e.key === 'End') {
        e.preventDefault();
        persistWidth(max);
      }
    },
    [max, min, paneWidth, persistWidth, policy.step, policy.stepLarge],
  );

  const resetWidth = useCallback(() => {
    persistWidth(policy.defaultWidth);
  }, [persistWidth, policy.defaultWidth]);

  return {
    width: collapsed ? policy.collapsedWidth : paneWidth,
    paneWidth,
    resizing,
    valuemin: Number.isFinite(min) ? min : policy.minPx,
    valuemax: Number.isFinite(max) ? max : undefined,
    widthTransition:
      !resizing && !reduceMotion ? 'transition-[width] duration-200 ease-in-out' : 'transition-none',
    onResizeStart,
    onSeparatorKeyDown,
    resetWidth,
  };
}

export type NavWidthController = ReturnType<typeof useNavWidth>;

export function useSidebarWidth(collapsed: boolean) {
  return useNavWidth({
    collapsed,
    storageKey: StorageKey.sidebarWidth,
    policy: PRIMARY_NAV_WIDTH,
  });
}
