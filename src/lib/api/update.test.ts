import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { resetBackend, setBackend } from '@/app/runtime';
import { createBackend as createMockBackend } from '@/dev/mocks/create-backend';
import { __setMockAvailableUpdate } from '@/dev/mocks/update';
import { packageAppVersion } from '@/lib/app-version';
import {
  checkForUpdate,
  downloadAndInstallUpdate,
  getAppVersion,
  isUpdateAvailable,
} from '@/lib/api/update';

describe('update API (mock backend)', () => {
  const current = packageAppVersion();

  beforeEach(() => {
    setBackend(createMockBackend());
    __setMockAvailableUpdate(null);
  });

  afterEach(() => {
    __setMockAvailableUpdate(null);
    resetBackend();
  });

  it('reports update capability and current version', async () => {
    await expect(isUpdateAvailable()).resolves.toBe(true);
    await expect(getAppVersion()).resolves.toBe(current);
  });

  it('returns null when already up to date', async () => {
    await expect(checkForUpdate()).resolves.toBeNull();
  });

  it('surfaces a pending update and completes one-click install', async () => {
    __setMockAvailableUpdate({
      version: '9.9.9',
      currentVersion: current,
      notes: 'test release',
      date: null,
    });
    const info = await checkForUpdate();
    expect(info).toMatchObject({ version: '9.9.9', currentVersion: current });

    const percents: number[] = [];
    await downloadAndInstallUpdate((p) => {
      if (p.percent != null) percents.push(p.percent);
    });
    expect(percents.length).toBeGreaterThan(0);
    expect(percents[percents.length - 1]).toBe(100);
    // After install mock clears the forced update.
    await expect(checkForUpdate()).resolves.toBeNull();
  });

  it('rejects install when no update is available', async () => {
    await expect(downloadAndInstallUpdate()).rejects.toThrow(/最新版本/);
  });
});
