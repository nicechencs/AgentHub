/**
 * UI list order — persist id sequences in localStorage and apply them to
 * live rows. Unknown ids stay at the end; filtered views reorder only the
 * visible subset without dropping hidden ids.
 */
import { loadJson, saveJson } from '@/lib/ui-preferences';

export function readIdOrder(storageKey: string): string[] {
  const raw = loadJson<unknown>(storageKey, []);
  if (!Array.isArray(raw)) return [];
  return raw.filter((id): id is string => typeof id === 'string' && id.length > 0);
}

const orderListeners = new Map<string, Set<() => void>>();

export function persistIdOrder(storageKey: string, ids: readonly string[]): void {
  saveJson(storageKey, [...ids]);
  const listeners = orderListeners.get(storageKey);
  if (!listeners) return;
  for (const listener of listeners) listener();
}

/** Live-sync the same storage key across mounted lists (Agents page + sidebar). */
export function subscribeIdOrder(storageKey: string, listener: () => void): () => void {
  let listeners = orderListeners.get(storageKey);
  if (!listeners) {
    listeners = new Set();
    orderListeners.set(storageKey, listeners);
  }
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
    if (listeners.size === 0) orderListeners.delete(storageKey);
  };
}

export function applyIdOrder<T>(
  items: readonly T[],
  getId: (item: T) => string,
  stored: readonly string[],
): T[] {
  if (items.length <= 1 || stored.length === 0) return [...items];
  const byId = new Map<string, T>();
  for (const item of items) {
    const id = getId(item);
    if (!byId.has(id)) byId.set(id, item);
  }
  const seen = new Set<string>();
  const out: T[] = [];
  for (const id of stored) {
    const item = byId.get(id);
    if (!item || seen.has(id)) continue;
    out.push(item);
    seen.add(id);
  }
  for (const item of items) {
    const id = getId(item);
    if (seen.has(id)) continue;
    out.push(item);
    seen.add(id);
  }
  return out;
}

export function moveId(order: readonly string[], fromId: string, toId: string): string[] {
  if (fromId === toId) return [...order];
  const from = order.indexOf(fromId);
  const to = order.indexOf(toId);
  if (from < 0 || to < 0) return [...order];
  const next = [...order];
  const [item] = next.splice(from, 1);
  next.splice(to, 0, item);
  return next;
}

/**
 * Reorder `fromId` to `toId` among `liveIds`, then write that live sequence
 * back into `stored` so hidden/filtered ids keep their relative place.
 */
export function mergeLiveMove(
  stored: readonly string[],
  liveIds: readonly string[],
  fromId: string,
  toId: string,
): string[] {
  const liveOrdered = applyIdOrder(liveIds, (id) => id, stored);
  const nextLive = moveId(liveOrdered, fromId, toId);
  if (nextLive.length === 0) return [...stored];
  const liveSet = new Set(liveIds);
  const result: string[] = [];
  const used = new Set<string>();
  let liveIndex = 0;
  const base = stored.length > 0 ? stored : liveIds;
  for (const id of base) {
    if (liveSet.has(id)) {
      const nextId = nextLive[liveIndex++];
      if (nextId && !used.has(nextId)) {
        result.push(nextId);
        used.add(nextId);
      }
    } else if (!used.has(id)) {
      result.push(id);
      used.add(id);
    }
  }
  for (const id of nextLive) {
    if (used.has(id)) continue;
    result.push(id);
    used.add(id);
  }
  return result;
}
