/**
 * Shared AgentHub self-update availability.
 * Populated by UpdatePrompt (auto-check / manual check); consumed by Sidebar
 * badge and Settings → About without re-fetching.
 */
import type { UpdateInfo } from '@/lib/backend/contracts/update-types';

type Listener = () => void;

let available: UpdateInfo | null = null;
const listeners = new Set<Listener>();

function emit(): void {
  for (const listener of listeners) listener();
}

export function getAppUpdateAvailable(): UpdateInfo | null {
  return available;
}

/** Publish or clear the pending app update (null = up to date / unknown). */
export function setAppUpdateAvailable(info: UpdateInfo | null): void {
  // Skip emit when the same version is re-published (auto + manual check).
  if (available === info) return;
  if (
    available &&
    info &&
    available.version === info.version &&
    available.currentVersion === info.currentVersion &&
    available.notes === info.notes
  ) {
    return;
  }
  available = info;
  emit();
}

export function subscribeAppUpdate(listener: Listener): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function resetAppUpdateStore(): void {
  available = null;
  emit();
}
