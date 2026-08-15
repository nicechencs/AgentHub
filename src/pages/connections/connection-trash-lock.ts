import { beginExclusiveBusyIds, endExclusiveBusyIds } from './connection-model';

type Listener = () => void;

let snapshot: ReadonlySet<string> = new Set();
const listeners = new Set<Listener>();

function emit(): void {
  for (const listener of listeners) listener();
}

export function getConnectionTrashBusyIds(): ReadonlySet<string> {
  return snapshot;
}

export function subscribeConnectionTrashBusy(listener: Listener): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** Process-wide exclusive lock so two recycle-bin instances cannot interleave. */
export function claimConnectionTrashBusy(id: string): boolean {
  const next = beginExclusiveBusyIds(snapshot, id);
  if (!next) return false;
  snapshot = next;
  emit();
  return true;
}

export function releaseConnectionTrashBusy(id: string): void {
  if (!snapshot.has(id)) return;
  snapshot = endExclusiveBusyIds(snapshot, id);
  emit();
}

export function resetConnectionTrashBusy(): void {
  if (snapshot.size === 0) return;
  snapshot = new Set();
  emit();
}
