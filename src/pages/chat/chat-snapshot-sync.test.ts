import { describe, expect, it } from 'vitest';
import {
  beginSnapshotSyncRetry,
  idleSnapshotSync,
  recordSnapshotPollFailure,
  recordSnapshotPollSuccess,
  SNAPSHOT_BANNER_AFTER_MS,
  snapshotSyncBannerDelayMs,
  snapshotSyncForConversation,
  snapshotSyncShowsBanner,
} from './chat-snapshot-sync';

describe('snapshot poll failure', () => {
  it('does not show a banner on the first instant failure', () => {
    const failed = recordSnapshotPollFailure(idleSnapshotSync(), 'chat-a', new Error('down'), 1_000);
    expect(failed.failures).toBe(1);
    expect(snapshotSyncShowsBanner(failed, 1_000)).toBe(false);
    expect(snapshotSyncBannerDelayMs(failed, 1_000)).toBe(SNAPSHOT_BANNER_AFTER_MS);
  });

  it('shows one banner after the delay and clears it on success', () => {
    const first = recordSnapshotPollFailure(idleSnapshotSync(), 'chat-a', new Error('down'), 1_000);
    const second = recordSnapshotPollFailure(first, 'chat-a', new Error('still down'), 1_080);
    expect(second.failures).toBe(2);
    expect(snapshotSyncShowsBanner(second, 1_080)).toBe(false);
    expect(snapshotSyncShowsBanner(second, 1_000 + SNAPSHOT_BANNER_AFTER_MS)).toBe(true);
    const retrying = beginSnapshotSyncRetry(second);
    expect(retrying.retrying).toBe(true);
    const recovered = recordSnapshotPollSuccess(retrying, 'chat-a');
    expect(recovered.failures).toBe(0);
    expect(snapshotSyncShowsBanner(recovered, 9_000)).toBe(false);
    expect(recordSnapshotPollSuccess(second, 'chat-b').failures).toBe(2);
  });

  it('resets when switching conversations', () => {
    const failed = recordSnapshotPollFailure(idleSnapshotSync(), 'chat-a', new Error('down'), 1_000);
    const switched = snapshotSyncForConversation('chat-b', failed);
    expect(switched.failures).toBe(0);
    expect(switched.conversationId).toBe('chat-b');
    expect(snapshotSyncForConversation('chat-a', failed)).toBe(failed);
  });
});
