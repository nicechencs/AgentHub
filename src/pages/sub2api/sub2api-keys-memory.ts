import type { Sub2ApiGroup, Sub2ApiKey } from '@/lib/sub2api';

/** Last Sub2API key list kept in memory so switching pages does not flash an empty table. */
export type Sub2ApiKeysMemory = {
  siteUrl: string;
  userKey: string;
  keys: Sub2ApiKey[];
  groups: Sub2ApiGroup[];
};

let memory: Sub2ApiKeysMemory | null = null;

export function readSub2ApiKeysMemory(): Sub2ApiKeysMemory | null {
  return memory;
}

export function writeSub2ApiKeysMemory(next: Sub2ApiKeysMemory): void {
  memory = next;
}

export function clearSub2ApiKeysMemory(): void {
  memory = null;
}
