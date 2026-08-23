import { describe, expect, it } from 'vitest';
import type { AppSettings } from '@/lib/types';
import {
  areLatestSettingsFields,
  createSettingsPersistenceTracker,
  mergeSettingsResponse,
} from './settings-persist';

const baseSettings: AppSettings = {
  language: 'zh',
  theme: 'light',
  autoStart: true,
  closeToTray: true,
  dataDir: '/data',
  logsDir: '/logs',
  logLevel: 'info',
  logRetentionDays: 14,
  skillMarketSource: 'auto',
  usageCollectIntervalMin: 30,
  appVersion: 'test',
};

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((next) => {
    resolve = next;
  });
  return { promise, resolve };
}

describe('settings persistence concurrency', () => {
  it('uses an older confirmed response when the newer same-field save fails', () => {
    const tracker = createSettingsPersistenceTracker();
    const first = tracker.begin({ theme: 'dark' }, baseSettings);
    const second = tracker.begin({ theme: 'system' }, { ...baseSettings, theme: 'dark' });

    const firstSuccess = tracker.settleSuccess(first, { ...baseSettings, theme: 'dark' }, ['theme']);
    expect(firstSuccess.ownedKeys).toEqual([]);
    expect(firstSuccess.committedPatch.theme).toBe('dark');

    const secondFailure = tracker.settleFailure(second);
    expect(secondFailure.ownedKeys).toEqual(['theme']);
    expect(secondFailure.rollbackPatch.theme).toBe('dark');
  });

  it('lets an older request commit after the newer request fails first', () => {
    const tracker = createSettingsPersistenceTracker();
    const first = tracker.begin({ language: 'en' }, baseSettings);
    const second = tracker.begin({ language: 'zh' }, { ...baseSettings, language: 'en' });

    const secondFailure = tracker.settleFailure(second);
    expect(secondFailure.rollbackPatch.language).toBe('zh');

    const firstSuccess = tracker.settleSuccess(first, { ...baseSettings, language: 'en' }, ['language']);
    expect(firstSuccess.ownedKeys).toEqual(['language']);
    expect(firstSuccess.committedPatch.language).toBe('en');
  });

  it('does not let an older response overwrite a newer confirmed write', () => {
    const tracker = createSettingsPersistenceTracker();
    const first = tracker.begin({ theme: 'dark' }, baseSettings);
    const second = tracker.begin({ theme: 'system' }, { ...baseSettings, theme: 'dark' });

    const secondSuccess = tracker.settleSuccess(
      second,
      { ...baseSettings, theme: 'system' },
      ['theme'],
    );
    expect(secondSuccess.ownedKeys).toEqual(['theme']);

    const firstSuccess = tracker.settleSuccess(
      first,
      { ...baseSettings, theme: 'dark' },
      ['theme'],
    );
    expect(firstSuccess.ownedKeys).toEqual([]);
    expect(firstSuccess.committedPatch.theme).toBeUndefined();
  });

  it('commits only requested fields from a full backend response', () => {
    const current = { ...baseSettings, theme: 'dark' as const, language: 'en' as const };
    const staleFullResponse = { ...baseSettings, theme: 'light' as const, language: 'zh' as const };

    expect(mergeSettingsResponse(current, staleFullResponse, { language: 'en' })).toEqual({
      ...current,
      language: 'zh',
    });
  });

  it('commits a post-write fallback snapshot instead of rolling the field back', () => {
    const tracker = createSettingsPersistenceTracker();
    const generation = tracker.begin({ closeToTray: false }, baseSettings);
    const fallback = { ...baseSettings, closeToTray: false, dataDir: '/data' };
    const success = tracker.settleSuccess(generation, fallback, ['closeToTray']);
    expect(success.ownedKeys).toEqual(['closeToTray']);
    expect(success.committedPatch.closeToTray).toBe(false);
    expect(success.rollbackPatch.closeToTray).toBe(false);
  });

  it('keeps field ownership when unrelated deferred saves complete in reverse order', async () => {
    let generation = 0;
    const committed: AppSettings[] = [];
    const fieldGenerations: Partial<Record<keyof AppSettings, number>> = {};
    const first = deferred<AppSettings>();
    const second = deferred<AppSettings>();
    const firstGeneration = ++generation;
    fieldGenerations.theme = firstGeneration;
    const secondGeneration = ++generation;
    fieldGenerations.language = secondGeneration;

    const commit = async (
      requestGeneration: number,
      result: Promise<AppSettings>,
      patch: Partial<AppSettings>,
    ) => {
      const saved = await result;
      const requestedKeys = Object.keys(patch) as Array<keyof AppSettings>;
      if (areLatestSettingsFields(fieldGenerations, requestGeneration, requestedKeys)) {
        const current = committed.at(-1) ?? baseSettings;
        committed.push(mergeSettingsResponse(current, saved, patch));
      }
    };

    const firstCommit = commit(firstGeneration, first.promise, { theme: 'dark' });
    const secondCommit = commit(secondGeneration, second.promise, { language: 'en' });
    second.resolve({ ...baseSettings, language: 'en' });
    await secondCommit;
    first.resolve({ ...baseSettings, theme: 'dark' });
    await firstCommit;

    expect(committed).toHaveLength(2);
    expect(committed.at(-1)).toEqual({ ...baseSettings, language: 'en', theme: 'dark' });
  });

  it('ignores an older deferred response when the same field was saved again', async () => {
    const first = deferred<AppSettings>();
    const second = deferred<AppSettings>();
    const fieldGenerations: Partial<Record<keyof AppSettings, number>> = {
      theme: 2,
    };
    const committed: AppSettings[] = [];
    const commit = async (requestGeneration: number, result: Promise<AppSettings>) => {
      const saved = await result;
      if (areLatestSettingsFields(fieldGenerations, requestGeneration, ['theme'])) {
        committed.push(mergeSettingsResponse(baseSettings, saved, { theme: saved.theme }));
      }
    };

    const firstCommit = commit(1, first.promise);
    const secondCommit = commit(2, second.promise);
    second.resolve({ ...baseSettings, theme: 'dark' });
    await secondCommit;
    first.resolve({ ...baseSettings, theme: 'system' });
    await firstCommit;

    expect(committed).toHaveLength(1);
    expect(committed[0].theme).toBe('dark');
  });
});
