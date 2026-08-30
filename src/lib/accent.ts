import {
  DEFAULT_ACCENT_ID,
  isAccentId,
  type AccentId,
} from '@/styles/tokens';
import { loadString, saveString, StorageKey } from '@/lib/ui-preferences';

export type { AccentId };

type ShellIconSync = (id: AccentId) => void;
let shellIconSync: ShellIconSync | undefined;

/** Composition root (boot) registers the desktop window/tray icon updater. */
export function registerShellIconSync(fn: ShellIconSync | null): void {
  shellIconSync = fn ?? undefined;
}

export function loadStoredAccent(): AccentId {
  const value = loadString(StorageKey.accent, DEFAULT_ACCENT_ID);
  return isAccentId(value) ? value : DEFAULT_ACCENT_ID;
}

/** Write the selected accent onto `<html data-accent>`. */
export function applyAccent(id: AccentId): void {
  if (typeof document === 'undefined') return;
  document.documentElement.dataset.accent = id;
}

export function persistAccent(id: AccentId): void {
  saveString(StorageKey.accent, id);
  applyAccent(id);
  shellIconSync?.(id);
}
