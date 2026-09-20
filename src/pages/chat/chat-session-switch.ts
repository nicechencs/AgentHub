export type SessionSwitchDirection = 'prev' | 'next';

export type SessionSwitchNeighbors = {
  prevId: string | null;
  nextId: string | null;
};

/**
 * Previous / next session in the current filtered list.
 * Wraps at both ends. A single-item list cannot switch.
 */
export function adjacentSessionId(
  sessions: readonly { id: string }[],
  currentId: string | null,
  direction: SessionSwitchDirection,
): string | null {
  if (sessions.length === 0) return null;
  const index = currentId == null ? -1 : sessions.findIndex((item) => item.id === currentId);
  let nextIndex: number;
  if (index < 0) {
    nextIndex = direction === 'next' ? 0 : sessions.length - 1;
  } else if (sessions.length === 1) {
    return null;
  } else {
    const delta = direction === 'next' ? 1 : -1;
    nextIndex = (index + delta + sessions.length) % sessions.length;
  }
  const target = sessions[nextIndex]?.id ?? null;
  return target && target !== currentId ? target : null;
}

export function sessionSwitchNeighbors(
  sessions: readonly { id: string }[],
  currentId: string | null,
): SessionSwitchNeighbors {
  return {
    prevId: adjacentSessionId(sessions, currentId, 'prev'),
    nextId: adjacentSessionId(sessions, currentId, 'next'),
  };
}

/** Alt+↑ / Alt+↓. Leaves Ctrl+K, Ctrl+N, Enter, Esc, and / alone. */
export function chatSessionSwitchShortcutAction(input: {
  key: string;
  code?: string;
  altKey: boolean;
  metaKey: boolean;
  ctrlKey: boolean;
  shiftKey: boolean;
  overlayOpen: boolean;
}): SessionSwitchDirection | null {
  if (input.overlayOpen || !input.altKey || input.metaKey || input.ctrlKey || input.shiftKey) {
    return null;
  }
  if (input.key === 'ArrowUp' || input.code === 'ArrowUp') return 'prev';
  if (input.key === 'ArrowDown' || input.code === 'ArrowDown') return 'next';
  return null;
}
