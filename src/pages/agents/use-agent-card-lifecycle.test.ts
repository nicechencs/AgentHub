import { describe, expect, it, vi } from 'vitest';
import { createInstallProgressSubscription } from './use-agent-card-lifecycle';

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
