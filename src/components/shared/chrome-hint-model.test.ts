import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { StorageKey } from '@/lib/storage-key';
import {
  dismissChromeHint,
  isChromeHintPending,
  notifyOnboardingFinished,
  subscribeChromeHint,
} from './chrome-hint-model';

const store = new Map<string, string>();

function stubStorage() {
  vi.stubGlobal('localStorage', {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => {
      store.set(key, value);
    },
    removeItem: (key: string) => {
      store.delete(key);
    },
  });
}

describe('chrome hint', () => {
  beforeEach(() => {
    store.clear();
    stubStorage();
  });

  afterEach(() => {
    store.clear();
    vi.unstubAllGlobals();
  });

  it('stays hidden until first-run onboarding is done', () => {
    expect(isChromeHintPending()).toBe(false);
    localStorage.setItem(StorageKey.onboardingDone, '1');
    expect(isChromeHintPending()).toBe(true);
  });

  it('does not come back after dismiss', () => {
    localStorage.setItem(StorageKey.onboardingDone, '1');
    dismissChromeHint();
    expect(isChromeHintPending()).toBe(false);
    dismissChromeHint();
    expect(isChromeHintPending()).toBe(false);
  });

  it('notifies subscribers when onboarding finishes', () => {
    const seen: boolean[] = [];
    const unsub = subscribeChromeHint(() => seen.push(isChromeHintPending()));
    localStorage.setItem(StorageKey.onboardingDone, '1');
    notifyOnboardingFinished();
    unsub();
    expect(seen).toEqual([true]);
  });
});
