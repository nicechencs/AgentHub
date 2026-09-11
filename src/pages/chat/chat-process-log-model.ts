import { readStorageItem, removeStorageItem, StorageKey } from '@/lib/storage-key';

export type ProcessLogPane = 'command' | 'stderr';

export const PROCESS_LOG_HEIGHT_STEP = 16;
export const PROCESS_LOG_HEIGHT_STEP_LARGE = 48;

export const PROCESS_LOG_SPECS = {
  command: {
    storageKey: StorageKey.chatProcessCommandHeight,
    defaultHeight: 160,
    minHeight: 72,
    maxHeight: 640,
  },
  stderr: {
    storageKey: StorageKey.chatProcessLogHeight,
    defaultHeight: 280,
    minHeight: 120,
    maxHeight: 720,
  },
} as const;

export function clampProcessLogHeight(pane: ProcessLogPane, height: number): number {
  const spec = PROCESS_LOG_SPECS[pane];
  const requested = Math.round(Number.isFinite(height) ? height : spec.defaultHeight);
  return Math.min(spec.maxHeight, Math.max(spec.minHeight, requested));
}

export function readStoredProcessLogHeight(pane: ProcessLogPane): number | null {
  if (typeof window === 'undefined') return null;
  try {
    const raw = readStorageItem(window.localStorage, PROCESS_LOG_SPECS[pane].storageKey);
    if (raw == null || raw === '') return null;
    const n = Number(raw);
    if (Number.isFinite(n) && n > 0) return clampProcessLogHeight(pane, n);
  } catch {
    /* ignore */
  }
  return null;
}

export function persistProcessLogHeight(pane: ProcessLogPane, height: number | null): void {
  const key = PROCESS_LOG_SPECS[pane].storageKey;
  try {
    if (height == null) {
      removeStorageItem(window.localStorage, key);
      return;
    }
    window.localStorage.setItem(key, String(clampProcessLogHeight(pane, height)));
  } catch {
    /* ignore */
  }
}

export function processLogHeightOrDefault(pane: ProcessLogPane, height: number | null): number {
  return height == null
    ? PROCESS_LOG_SPECS[pane].defaultHeight
    : clampProcessLogHeight(pane, height);
}
