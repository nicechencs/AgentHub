import { afterEach, describe, expect, it } from 'vitest';
import {
  getAppUpdateAvailable,
  resetAppUpdateStore,
  setAppUpdateAvailable,
  subscribeAppUpdate,
} from './app-update-store';
import type { UpdateInfo } from '@/lib/backend/contracts/update-types';

function info(version: string, current = '0.1.0'): UpdateInfo {
  return {
    version,
    currentVersion: current,
    notes: `notes for ${version}`,
    date: null,
  };
}

describe('app-update-store', () => {
  afterEach(() => {
    resetAppUpdateStore();
  });

  it('starts empty and publishes pending updates', () => {
    expect(getAppUpdateAvailable()).toBeNull();
    setAppUpdateAvailable(info('1.2.3'));
    expect(getAppUpdateAvailable()?.version).toBe('1.2.3');
    setAppUpdateAvailable(null);
    expect(getAppUpdateAvailable()).toBeNull();
  });

  it('notifies subscribers on change', () => {
    let n = 0;
    const unsub = subscribeAppUpdate(() => {
      n += 1;
    });
    setAppUpdateAvailable(info('2.0.0'));
    setAppUpdateAvailable(info('2.0.0')); // same version → no extra emit
    setAppUpdateAvailable(info('2.0.1'));
    unsub();
    setAppUpdateAvailable(null);
    expect(n).toBe(2);
  });
});
