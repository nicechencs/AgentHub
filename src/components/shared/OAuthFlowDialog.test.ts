import { describe, expect, it, vi } from 'vitest';

import {
  createOAuthFlowToken,
  isOAuthFlowTokenCurrent,
  openManualCallbackFallbackIfCurrent,
  type OAuthFlowToken,
} from './OAuthFlowDialog';

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((next) => {
    resolve = next;
  });
  return { promise, resolve };
}

describe('OAuth flow identity', () => {
  it('ignores a completion from a closed and reopened dialog', async () => {
    let current: OAuthFlowToken | null = createOAuthFlowToken(1);
    const pending = deferred<'old-complete'>();
    const old = current;
    current.cancelled = true;
    current = createOAuthFlowToken(2);

    pending.resolve('old-complete');
    await pending.promise;

    expect(isOAuthFlowTokenCurrent(current, old)).toBe(false);
    expect(isOAuthFlowTokenCurrent(current, current)).toBe(true);
  });

  it('ignores a completion after switching agents', async () => {
    let current: OAuthFlowToken | null = createOAuthFlowToken(10);
    const pending = deferred<'old-agent-complete'>();
    const old = current;
    old.cancelled = true;
    current = createOAuthFlowToken(11);

    pending.resolve('old-agent-complete');
    await pending.promise;

    expect(isOAuthFlowTokenCurrent(current, old)).toBe(false);
  });

  it('does not open the old manual callback URL after the flow becomes stale', async () => {
    let current: OAuthFlowToken | null = createOAuthFlowToken(20);
    const old = current;
    old.cancelled = true;
    current = createOAuthFlowToken(21);
    const openLink = vi.fn(async () => {});

    await openManualCallbackFallbackIfCurrent(
      'https://old.example/callback',
      () => isOAuthFlowTokenCurrent(current, old),
      openLink,
    );

    expect(openLink).not.toHaveBeenCalled();
  });
});
