/** Stack of page titles so a kept-alive page does not lose the top bar. */

export type PageChromeFields = {
  title: string;
  description?: string;
  descriptionTip?: string;
};

export type PageChromeEntry<T extends PageChromeFields = PageChromeFields> = {
  id: number;
  chrome: T;
};

export function upsertPageChromeEntry<T extends PageChromeFields>(
  entries: readonly PageChromeEntry<T>[],
  id: number,
  chrome: T,
): PageChromeEntry<T>[] {
  const index = entries.findIndex((entry) => entry.id === id);
  if (index === -1) return [...entries, { id, chrome }];
  const next = entries.slice();
  next[index] = { id, chrome };
  return next;
}

export function removePageChromeEntry<T extends PageChromeFields>(
  entries: readonly PageChromeEntry<T>[],
  id: number,
): PageChromeEntry<T>[] {
  return entries.filter((entry) => entry.id !== id);
}

export function topPageChrome<T extends PageChromeFields>(
  entries: readonly PageChromeEntry<T>[],
): T | null {
  return entries.length ? entries[entries.length - 1].chrome : null;
}
