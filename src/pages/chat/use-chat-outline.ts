import { useCallback, useEffect, useState } from 'react';
import { outlinePanelElement, readOutlinePanelWidth } from './chat-outline-model';
import {
  loadChatOutlineEnabled,
  subscribeChatOutlineEnabled,
} from './chat-outline-pref';

export function useChatOutlineEnabled(override?: boolean): boolean {
  const [enabled, setEnabled] = useState(loadChatOutlineEnabled);
  useEffect(() => subscribeChatOutlineEnabled(setEnabled), []);
  return override ?? enabled;
}

/**
 * Measure the chat stage (wide panel). A numeric override is for tests / SSR
 * so the rail can mount without ResizeObserver.
 */
export function useOutlinePanelWidth(enabled: boolean, override?: number): {
  width: number;
  assignRef: (node: HTMLDivElement | null) => void;
} {
  const [hostNode, setHostNode] = useState<HTMLDivElement | null>(null);
  const [observed, setObserved] = useState(0);

  const assignRef = useCallback((node: HTMLDivElement | null) => {
    setHostNode((prev) => (prev === node ? prev : node));
  }, []);

  useEffect(() => {
    if (override != null || !enabled) return;
    const node = hostNode;
    if (!node) return;
    const apply = () => {
      const next = readOutlinePanelWidth(node);
      setObserved((prev) => (prev === next ? prev : next));
    };
    apply();
    if (typeof ResizeObserver === 'undefined') {
      window.addEventListener('resize', apply);
      return () => window.removeEventListener('resize', apply);
    }
    const observer = new ResizeObserver(apply);
    observer.observe(outlinePanelElement(node) ?? node);
    return () => observer.disconnect();
  }, [enabled, hostNode, override]);

  return { width: override ?? observed, assignRef };
}
