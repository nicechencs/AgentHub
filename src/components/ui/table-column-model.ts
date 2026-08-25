import { loadJson, saveJson } from '@/lib/ui-preferences';

/** Merge persisted column widths onto the current spec; unknown keys are dropped. */
export function mergeStoredColumnWidths<K extends string>(
  stored: unknown,
  defaults: Record<K, number>,
  minByKey: Record<K, number>,
): Record<K, number> {
  const next = { ...defaults };
  if (!stored || typeof stored !== 'object' || Array.isArray(stored)) return next;
  const record = stored as Record<string, unknown>;
  for (const key of Object.keys(defaults) as K[]) {
    const n = Number(record[key]);
    if (!Number.isFinite(n)) continue;
    const min = minByKey[key] ?? 48;
    next[key] = Math.max(min, Math.round(n));
  }
  return next;
}

export function readStoredColumnWidths<K extends string>(
  storageKey: string,
  defaults: Record<K, number>,
  minByKey: Record<K, number>,
): Record<K, number> {
  return mergeStoredColumnWidths(loadJson<unknown>(storageKey, null), defaults, minByKey);
}

export function persistColumnWidths<K extends string>(
  storageKey: string,
  widths: Record<K, number>,
): void {
  saveJson(storageKey, widths);
}
