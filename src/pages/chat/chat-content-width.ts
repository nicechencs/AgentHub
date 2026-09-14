/**
 * Chat transcript content-width axis (aligned with Deepseek Harness).
 *
 * Adaptive default: clamp(680px, 64% of column, 920px).
 * Dragged preference replaces the adaptive term and persists in localStorage.
 * Display re-clamps when the column shrinks without rewriting storage.
 */

export const CHAT_CONTENT_WIDTH_MIN = 640;
/** Leave 88px per side so width handles stay placeable. */
export const CHAT_CONTENT_WIDTH_EDGE_BUDGET = 176;
export const CHAT_CONTENT_WIDTH_ADAPTIVE_FLOOR = 680;
export const CHAT_CONTENT_WIDTH_ADAPTIVE_RATIO = 0.64;
export const CHAT_CONTENT_WIDTH_ADAPTIVE_CAP = 920;

export function readChatContentWidthPreference(
  storage: Pick<Storage, 'getItem'> | null | undefined = typeof localStorage === 'undefined'
    ? null
    : localStorage,
  key: string,
): number | null {
  if (!storage) return null;
  try {
    const raw = storage.getItem(key);
    if (raw == null) return null;
    const value = Number(raw);
    return Number.isFinite(value) && value > 0 ? value : null;
  } catch {
    return null;
  }
}

export function writeChatContentWidthPreference(
  storage: Pick<Storage, 'setItem'> | null | undefined,
  key: string,
  width: number,
): void {
  if (!storage) return;
  try {
    storage.setItem(key, `${Math.round(width)}`);
  } catch {
    /* ignore quota / private mode */
  }
}

/** Mirrors the CSS clamp used by the conversation column. */
export function resolveChatContentWidth(
  columnWidth: number,
  preference: number | null,
): number {
  const column = Number.isFinite(columnWidth) ? Math.max(0, columnWidth) : 0;
  const max = Math.max(CHAT_CONTENT_WIDTH_MIN, column - CHAT_CONTENT_WIDTH_EDGE_BUDGET);
  if (preference != null) {
    return Math.min(Math.max(preference, CHAT_CONTENT_WIDTH_MIN), max);
  }
  return Math.max(
    CHAT_CONTENT_WIDTH_ADAPTIVE_FLOOR,
    Math.min(column * CHAT_CONTENT_WIDTH_ADAPTIVE_RATIO, CHAT_CONTENT_WIDTH_ADAPTIVE_CAP),
  );
}
