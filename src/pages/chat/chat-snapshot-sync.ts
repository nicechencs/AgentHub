/** Snapshot poll failures must not toast every 80ms. Keep the last view. */

export const SNAPSHOT_BANNER_AFTER_MS = 1000;

export type SnapshotSyncState = {
  conversationId: string | null;
  failures: number;
  firstFailedAt: number | null;
  lastError: string | null;
  retrying: boolean;
};

export function idleSnapshotSync(): SnapshotSyncState {
  return {
    conversationId: null,
    failures: 0,
    firstFailedAt: null,
    lastError: null,
    retrying: false,
  };
}

export function snapshotSyncForConversation(
  conversationId: string | null,
  previous: SnapshotSyncState = idleSnapshotSync(),
): SnapshotSyncState {
  if (previous.conversationId === conversationId) return previous;
  return { ...idleSnapshotSync(), conversationId };
}

export function recordSnapshotPollSuccess(
  state: SnapshotSyncState,
  conversationId: string,
): SnapshotSyncState {
  if (state.conversationId && state.conversationId !== conversationId) return state;
  return {
    conversationId,
    failures: 0,
    firstFailedAt: null,
    lastError: null,
    retrying: false,
  };
}

export function recordSnapshotPollFailure(
  state: SnapshotSyncState,
  conversationId: string,
  error: unknown,
  now: number,
): SnapshotSyncState {
  const same = state.conversationId === conversationId && state.failures > 0;
  const message = error instanceof Error ? error.message : String(error);
  return {
    conversationId,
    failures: same ? state.failures + 1 : 1,
    firstFailedAt: same ? state.firstFailedAt ?? now : now,
    lastError: message.trim() || null,
    retrying: false,
  };
}

export function beginSnapshotSyncRetry(state: SnapshotSyncState): SnapshotSyncState {
  if (!state.conversationId || state.failures === 0) return state;
  return { ...state, retrying: true };
}

export function snapshotSyncShowsBanner(state: SnapshotSyncState, now: number): boolean {
  if (!state.conversationId || state.failures === 0 || state.firstFailedAt == null) {
    return false;
  }
  return now - state.firstFailedAt >= SNAPSHOT_BANNER_AFTER_MS;
}

export function snapshotSyncBannerDelayMs(state: SnapshotSyncState, now: number): number | null {
  if (!state.conversationId || state.failures === 0 || state.firstFailedAt == null) {
    return null;
  }
  const remaining = SNAPSHOT_BANNER_AFTER_MS - (now - state.firstFailedAt);
  return remaining > 0 ? remaining : null;
}
