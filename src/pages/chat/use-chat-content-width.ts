/**
 * Chat column content-width: adaptive clamp + symmetric drag handles.
 * Persists preference; display re-clamps without rewriting storage.
 */
import { useCallback, useEffect, useRef, useState } from 'react';
import { StorageKey } from '@/lib/storage-key';
import {
  readChatContentWidthPreference,
  resolveChatContentWidth,
  writeChatContentWidthPreference,
} from './chat-content-width';

export function useChatContentWidth() {
  const rootRef = useRef<HTMLDivElement | null>(null);
  const [dragging, setDragging] = useState(false);
  const drag = useRef({
    side: 'right' as 'left' | 'right',
    originX: 0,
    baseWidth: 0,
    latestX: 0,
    frame: null as number | null,
  });

  const publish = useCallback((root: HTMLDivElement) => {
    const column = root.offsetWidth;
    root.style.setProperty('--ah-chat-column-width', `${column}px`);
    const preference = readChatContentWidthPreference(localStorage, StorageKey.chatContentWidth);
    if (preference == null) {
      root.style.removeProperty('--ah-chat-user-width');
    } else {
      root.style.setProperty(
        '--ah-chat-user-width',
        `${resolveChatContentWidth(column, preference)}px`,
      );
    }
  }, []);

  const rootCallbackRef = useCallback(
    (root: HTMLDivElement | null) => {
      rootRef.current = root;
      if (!root) return;
      publish(root);
    },
    [publish],
  );

  useEffect(() => {
    const root = rootRef.current;
    if (!root || typeof ResizeObserver === 'undefined') return;
    const observer = new ResizeObserver(() => {
      if (rootRef.current) publish(rootRef.current);
    });
    observer.observe(root);
    publish(root);
    return () => observer.disconnect();
  }, [publish]);

  const resolvedWidth = useCallback((): number => {
    const root = rootRef.current;
    if (!root) return 680;
    return resolveChatContentWidth(
      root.offsetWidth,
      readChatContentWidthPreference(localStorage, StorageKey.chatContentWidth),
    );
  }, []);

  const applyLiveWidth = useCallback((width: number) => {
    const root = rootRef.current;
    if (!root) return;
    const clamped = resolveChatContentWidth(root.offsetWidth, width);
    root.style.setProperty('--ah-chat-user-width', `${clamped}px`);
  }, []);

  const onPointerDown = useCallback(
    (side: 'left' | 'right') => (e: React.PointerEvent<HTMLDivElement>) => {
      e.preventDefault();
      e.currentTarget.setPointerCapture(e.pointerId);
      drag.current = {
        side,
        originX: e.clientX,
        latestX: e.clientX,
        baseWidth: resolvedWidth(),
        frame: null,
      };
      setDragging(true);
    },
    [resolvedWidth],
  );

  const outwardWidth = useCallback(() => {
    const dx = drag.current.latestX - drag.current.originX;
    const outward = drag.current.side === 'right' ? dx : -dx;
    return drag.current.baseWidth + outward * 2;
  }, []);

  const onPointerMove = useCallback(
    (e: React.PointerEvent<HTMLDivElement>) => {
      const box = e.currentTarget.getBoundingClientRect();
      e.currentTarget.style.setProperty('--ah-width-handle-pointer-y', `${e.clientY - box.top}px`);
      if (!e.currentTarget.hasPointerCapture(e.pointerId)) return;
      drag.current.latestX = e.clientX;
      if (drag.current.frame != null) return;
      drag.current.frame = requestAnimationFrame(() => {
        drag.current.frame = null;
        applyLiveWidth(outwardWidth());
      });
    },
    [applyLiveWidth, outwardWidth],
  );

  const finishDrag = useCallback(
    (commit: boolean) => {
      if (drag.current.frame != null) {
        cancelAnimationFrame(drag.current.frame);
        drag.current.frame = null;
      }
      const root = rootRef.current;
      if (commit && root && drag.current.latestX !== drag.current.originX) {
        const clamped = resolveChatContentWidth(root.offsetWidth, outwardWidth());
        writeChatContentWidthPreference(localStorage, StorageKey.chatContentWidth, clamped);
      }
      setDragging(false);
      if (root) publish(root);
    },
    [outwardWidth, publish],
  );

  const onPointerUp = useCallback(
    (e: React.PointerEvent<HTMLDivElement>) => {
      if (!e.currentTarget.hasPointerCapture(e.pointerId)) return;
      e.currentTarget.releasePointerCapture(e.pointerId);
      drag.current.latestX = e.clientX;
      finishDrag(true);
    },
    [finishDrag],
  );

  const onPointerCancel = useCallback(() => {
    finishDrag(false);
  }, [finishDrag]);

  return {
    rootRef: rootCallbackRef,
    dragging,
    onPointerDown,
    onPointerMove,
    onPointerUp,
    onPointerCancel,
  };
}
