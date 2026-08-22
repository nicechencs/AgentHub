import { describe, expect, it, vi } from 'vitest';
import {
  createInstallProgressSubscription,
  installOutputChunksToLines,
  recordInstallOutputChunk,
} from './use-agent-card-lifecycle';

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((next) => {
    resolve = next;
  });
  return { promise, resolve };
}

describe('install progress subscription lifecycle', () => {
  it('awaits subscription setup and cleans up a listener that resolves after disposal', async () => {
    const setup = deferred<() => void>();
    const lateUnsubscribe = vi.fn();
    const subscribe = vi.fn(() => setup.promise);
    const subscription = createInstallProgressSubscription(subscribe, () => {});

    subscription.dispose();
    setup.resolve(lateUnsubscribe);
    await subscription.ready;

    expect(subscribe).toHaveBeenCalledOnce();
    expect(lateUnsubscribe).toHaveBeenCalledOnce();
  });

  it('rejects setup failures instead of silently starting the command', async () => {
    const error = new Error('listen failed');
    const subscription = createInstallProgressSubscription(
      async () => {
        throw error;
      },
      () => {},
    );

    await expect(subscription.ready).rejects.toBe(error);
  });
});

describe('install output raw chunks', () => {
  it('keeps an empty chunk instead of dropping it', () => {
    const chunks: string[] = [];
    recordInstallOutputChunk(chunks, '');
    expect(chunks).toEqual(['']);
    expect(installOutputChunksToLines(chunks)).toEqual(['']);
  });

  it('keeps whitespace and multiple newlines and joins mid-line splits', () => {
    const chunks: string[] = [];
    recordInstallOutputChunk(chunks, '   ');
    recordInstallOutputChunk(chunks, '\n');
    recordInstallOutputChunk(chunks, '\n');
    recordInstallOutputChunk(chunks, 'hel');
    recordInstallOutputChunk(chunks, 'lo\n');
    recordInstallOutputChunk(chunks, 'wor');
    recordInstallOutputChunk(chunks, 'ld');

    expect(chunks).toEqual(['   ', '\n', '\n', 'hel', 'lo\n', 'wor', 'ld']);
    expect(installOutputChunksToLines(chunks)).toEqual(['   ', '', 'hello', 'world']);
  });

  it('does not trimEnd trailing spaces on a chunk', () => {
    const chunks: string[] = [];
    recordInstallOutputChunk(chunks, 'keep  ');
    expect(installOutputChunksToLines(chunks)).toEqual(['keep  ']);
  });
});
