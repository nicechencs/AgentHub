import { useEffect } from 'react';
import { hasEscPriorityOverlay } from '@/lib/skills/preview-keys';
import {
  adjacentSessionId,
  chatSessionSwitchShortcutAction,
} from './chat-session-switch';
import { subscribeChatShortcutKeydown } from './chat-shortcuts';

export function useChatSessionSwitch(input: {
  sessions: readonly { id: string }[];
  currentId: string | null;
  onFocus: (id: string) => void;
}): void {
  const { sessions, currentId, onFocus } = input;
  const idsKey = sessions.map((item) => item.id).join('\n');

  useEffect(() => {
    const list = idsKey === '' ? [] : idsKey.split('\n').map((id) => ({ id }));
    return subscribeChatShortcutKeydown((event) => {
      if (event.isComposing) return;
      const action = chatSessionSwitchShortcutAction({
        key: event.key,
        code: event.code,
        altKey: event.altKey,
        metaKey: event.metaKey,
        ctrlKey: event.ctrlKey,
        shiftKey: event.shiftKey,
        overlayOpen: hasEscPriorityOverlay(),
      });
      if (!action) return;
      const target = adjacentSessionId(list, currentId, action);
      if (!target) return;
      event.preventDefault();
      event.stopPropagation();
      onFocus(target);
    });
  }, [currentId, idsKey, onFocus]);
}
