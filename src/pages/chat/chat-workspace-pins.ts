import { loadJson, saveJson, StorageKey } from '@/lib/ui-preferences';

export function parseWorkspacePinKeys(raw: unknown): string[] {
  if (!Array.isArray(raw)) return [];
  const out: string[] = [];
  const seen = new Set<string>();
  for (const item of raw) {
    if (typeof item !== 'string') continue;
    const key = item.trim();
    if (!key || seen.has(key)) continue;
    seen.add(key);
    out.push(key);
  }
  return out;
}

/** Pin moves the key to the front; pinning again removes it. */
export function toggleWorkspacePinKeys(pins: readonly string[], key: string): string[] {
  const nextKey = key.trim();
  const current = parseWorkspacePinKeys(pins);
  if (!nextKey) return current;
  if (current.includes(nextKey)) return current.filter((item) => item !== nextKey);
  return [nextKey, ...current];
}

export function applyWorkspacePins<T extends { key: string }>(
  groups: readonly T[],
  pins: readonly string[],
): T[] {
  const order = parseWorkspacePinKeys(pins);
  if (groups.length === 0 || order.length === 0) return [...groups];
  const byKey = new Map<string, T>();
  for (const group of groups) {
    if (!byKey.has(group.key)) byKey.set(group.key, group);
  }
  const seen = new Set<string>();
  const out: T[] = [];
  for (const key of order) {
    const group = byKey.get(key);
    if (!group || seen.has(key)) continue;
    out.push(group);
    seen.add(key);
  }
  for (const group of groups) {
    if (seen.has(group.key)) continue;
    out.push(group);
    seen.add(group.key);
  }
  return out;
}

export function loadChatWorkspacePins(): string[] {
  return parseWorkspacePinKeys(loadJson<unknown>(StorageKey.chatWorkspacePins, []));
}

export function saveChatWorkspacePins(keys: readonly string[]): void {
  saveJson(StorageKey.chatWorkspacePins, parseWorkspacePinKeys(keys));
}
