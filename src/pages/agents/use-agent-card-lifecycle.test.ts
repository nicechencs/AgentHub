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

describe('install output chunk cap', () => {
  it('trims the chunk array to the cap while keeping the tail and line cap', () => {
    const chunks: string[] = [];
    for (let i = 0; i < 2500; i += 1) {
      recordInstallOutputChunk(chunks, `chunk-${i}\n`);
    }

    expect(chunks.length).toBe(2000);
    expect(chunks[0]).toBe('chunk-500\n');
    expect(chunks.at(-1)).toBe('chunk-2499\n');

    const lines = installOutputChunksToLines(chunks);
    expect(lines.length).toBeLessThanOrEqual(400);
    // Trailing '' from the final newline is part of the last-400 window.
    expect(lines[0]).toBe('chunk-2101');
    expect(lines.at(-2)).toBe('chunk-2499');
  });

  it('does not lose mid-line content across a head trim', () => {
    const chunks: string[] = [];
    recordInstallOutputChunk(chunks, 'head-that-will-be-trimmed ');
    for (let i = 0; i < 2000; i += 1) {
      recordInstallOutputChunk(chunks, `\nrow-${i}`);
    }
    // This chunk splits a line whose start lives in an already-retained
    // chunk; joining must still produce complete rows without duplication.
    recordInstallOutputChunk(chunks, '-suffix\n');
    expect(chunks.length).toBe(2000);
    expect(chunks[0]).toBe('\nrow-1');

    const lines = installOutputChunksToLines(chunks);
    expect(lines.at(-2)).toBe(`row-${1999}-suffix`);
    // No retained row was split by the head trim (the trailing '' comes from
    // the final newline and is expected).
    expect(lines.some((line) => line === '-suffix')).toBe(false);
  });
});
