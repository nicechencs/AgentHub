import { updateSettings } from '@/lib/api/settings';
import type { AppSettings } from '@/lib/types';

type SettingsKey = keyof AppSettings;

export type SettingsPersistenceSettlement = {
  ownedKeys: SettingsKey[];
  /** Values that have become the most recent confirmed backend values. */
  committedPatch: Partial<AppSettings>;
  /** Confirmed values to restore when the current request fails. */
  rollbackPatch: Partial<AppSettings>;
};

/**
 * Tracks optimistic settings writes by field.
 *
 * Requests are independent at the UI boundary, so an older successful write
 * must remain available as the rollback value if a newer write fails. A
 * response from an older generation is ignored once a newer success has been
 * observed for that field, but it is retained while a newer request is still
 * pending.
 */
export function createSettingsPersistenceTracker() {
  let generation = 0;
  const baseline: Partial<AppSettings> = {};
  const latestSuccessfulGeneration: Partial<Record<SettingsKey, number>> = {};
  const latestPendingGeneration: Partial<Record<SettingsKey, number>> = {};
  const pending: Partial<Record<SettingsKey, number[]>> = {};

  const has = (object: object, key: PropertyKey) =>
    Object.prototype.hasOwnProperty.call(object, key);

  const valueFor = (key: SettingsKey) =>
    has(baseline, key) ? baseline[key] : undefined;

  return {
    begin(patch: Partial<AppSettings>, initial: AppSettings): number {
      const requestGeneration = ++generation;
      for (const key of Object.keys(patch) as SettingsKey[]) {
        if (!has(baseline, key)) Object.assign(baseline, { [key]: initial[key] });
        const requests = pending[key] ?? [];
        requests.push(requestGeneration);
        pending[key] = requests;
        latestPendingGeneration[key] = requestGeneration;
      }
      return requestGeneration;
    },

    settleSuccess(
      requestGeneration: number,
      saved: AppSettings,
      requestedKeys: SettingsKey[],
    ): SettingsPersistenceSettlement {
      const committedPatch: Partial<AppSettings> = {};
      const ownedKeys: SettingsKey[] = [];

      for (const key of requestedKeys) {
        const successfulGeneration = latestSuccessfulGeneration[key] ?? -1;
        if (requestGeneration >= successfulGeneration && saved[key] !== undefined) {
          latestSuccessfulGeneration[key] = requestGeneration;
          Object.assign(baseline, { [key]: saved[key] });
          Object.assign(committedPatch, { [key]: saved[key] });
        }
      }

      for (const key of Object.keys(pending) as SettingsKey[]) {
        // A successful write supersedes every older in-flight write for the
        // same field. Older responses must not become eligible to overwrite
        // the newer confirmed value when they settle later.
        const requests = (pending[key] ?? []).filter((value) => value > requestGeneration);
        const ownsField = latestPendingGeneration[key] === requestGeneration;
        pending[key] = requests;
        if (ownsField) {
          ownedKeys.push(key);
          latestPendingGeneration[key] = requests.length > 0 ? requests[requests.length - 1] : 0;
        }
      }

      const rollbackPatch: Partial<AppSettings> = {};
      for (const key of ownedKeys) {
        const value = valueFor(key);
        if (value !== undefined) Object.assign(rollbackPatch, { [key]: value });
      }

      return { ownedKeys, committedPatch, rollbackPatch };
    },

    settleFailure(requestGeneration: number): SettingsPersistenceSettlement {
      const ownedKeys: SettingsKey[] = [];
      for (const key of Object.keys(pending) as SettingsKey[]) {
        const requests = (pending[key] ?? []).filter((value) => value !== requestGeneration);
        if (latestPendingGeneration[key] === requestGeneration) {
          ownedKeys.push(key);
          latestPendingGeneration[key] = requests.length > 0 ? requests[requests.length - 1] : 0;
        }
        pending[key] = requests;
      }

      const rollbackPatch: Partial<AppSettings> = {};
      for (const key of ownedKeys) {
        const value = valueFor(key);
        if (value !== undefined) Object.assign(rollbackPatch, { [key]: value });
      }

      return { ownedKeys, committedPatch: {}, rollbackPatch };
    },
  };
}

/** A response may only commit fields that its request actually changed. */
export function mergeSettingsResponse(
  current: AppSettings,
  saved: AppSettings,
  requestedPatch: Partial<AppSettings>,
): AppSettings {
  const next = { ...current };
  for (const key of Object.keys(requestedPatch) as Array<keyof AppSettings>) {
    const value = saved[key];
    if (value !== undefined) Object.assign(next, { [key]: value });
  }
  return next;
}

export function isLatestSettingsRequest(currentGeneration: number, requestGeneration: number) {
  return currentGeneration === requestGeneration;
}

export function areLatestSettingsFields(
  generations: Partial<Record<keyof AppSettings, number>>,
  requestGeneration: number,
  requestedKeys: Array<keyof AppSettings>,
) {
  return requestedKeys.every((key) => generations[key] === requestGeneration);
}

/**
 * Persist a settings patch immediately. Callers apply optimistic UI first;
 * the tracker above supplies confirmed values for success/failure handling.
 */
export async function persistSettingsPatch(
  patch: Partial<AppSettings>,
): Promise<AppSettings> {
  return updateSettings(patch);
}
