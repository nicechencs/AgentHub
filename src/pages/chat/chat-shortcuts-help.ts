export type ShortcutsHelpOpenReason = 'hover' | 'click' | null;

export type ShortcutsHelpEvent = 'hover-enter' | 'hover-leave' | 'click' | 'dismiss';

/** Hover opens; click pins the same panel; a second click or dismiss closes. */
export function shortcutsHelpOpenChange(
  current: ShortcutsHelpOpenReason,
  event: ShortcutsHelpEvent,
): ShortcutsHelpOpenReason {
  if (event === 'dismiss') return null;
  if (event === 'click') return current === 'click' ? null : 'click';
  if (event === 'hover-enter') return current === 'click' ? 'click' : 'hover';
  if (event === 'hover-leave') return current === 'hover' ? null : current;
  return current;
}

export function shortcutsHelpExpanded(reason: ShortcutsHelpOpenReason): boolean {
  return reason != null;
}
