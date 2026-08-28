import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it, vi } from 'vitest';

import {
  createOAuthFlowToken,
  isOAuthFlowTokenCurrent,
  openManualCallbackFallbackIfCurrent,
  validateManualCallbackUrl,
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

describe('validateManualCallbackUrl', () => {
  const redirect = 'http://127.0.0.1:1455/callback';
  const state = 'abc';

  it('accepts a loopback callback with matching state and code', () => {
    const got = validateManualCallbackUrl(
      `${redirect}?code=ok&state=${state}`,
      redirect,
      state,
    );
    expect(got.ok).toBe(true);
  });

  it('rejects a public URL even when it contains code=', () => {
    expect(
      validateManualCallbackUrl(
        `https://evil.example/steal?code=ok&state=${state}`,
        redirect,
        state,
      ).ok,
    ).toBe(false);
  });

  it('rejects a loopback URL with the wrong state', () => {
    expect(
      validateManualCallbackUrl(
        `${redirect}?code=ok&state=other`,
        redirect,
        state,
      ).ok,
    ).toBe(false);
  });
});

describe('official login wait page copy', () => {
  it('does not print raw session state or auth.json paths', () => {
    const src = readFileSync(
      path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../connect/OAuthFlowDialog.tsx'),
      'utf8',
    );
    expect(src).not.toContain('state: {');
    expect(src).not.toContain('auth.json');
    expect(src).not.toContain('~/.pi');
    expect(src).not.toContain('opt.authJsonKey');
    expect(src).toContain("officialLoginFooter(step");
    expect(src).toContain("t('chrome.error.retry')");
  });
});
