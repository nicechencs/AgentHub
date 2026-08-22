import { describe, expect, it } from 'vitest';
import { isLatestUsageRequest } from './usage-request';

describe('dashboard usage request concurrency', () => {
  it('allows only the newest deferred filter/event load to commit and finish loading', async () => {
    const deferred = <T,>() => {
      let resolve!: (value: T) => void;
      const promise = new Promise<T>((next) => {
        resolve = next;
      });
      return { promise, resolve };
    };
    let generation = 0;
    let loading = true;
    const committed: string[] = [];
    const first = deferred<string>();
    const second = deferred<string>();
    const firstGeneration = ++generation;
    const secondGeneration = ++generation;

    const consume = async (requestGeneration: number, result: Promise<string>) => {
      const value = await result;
      if (!isLatestUsageRequest(generation, requestGeneration)) return;
      committed.push(value);
      loading = false;
    };

    const firstConsume = consume(firstGeneration, first.promise);
    const secondConsume = consume(secondGeneration, second.promise);
    second.resolve('new-filter');
    await secondConsume;
    first.resolve('old-event');
    await firstConsume;

    expect(committed).toEqual(['new-filter']);
    expect(loading).toBe(false);
  });
});
