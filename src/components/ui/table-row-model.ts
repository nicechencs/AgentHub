/** Click/keyboard open for field-table rows. Ignores controls inside the row. */
export const INTERACTIVE_TABLE_TARGET_SELECTOR =
  'button, a, input, textarea, [role="button"], [role="switch"], [role="menuitem"]';

type ClosestTarget = {
  closest: (selector: string) => unknown;
};

function hasClosest(target: EventTarget | null): target is EventTarget & ClosestTarget {
  return Boolean(target && typeof (target as unknown as ClosestTarget).closest === 'function');
}

export function isInteractiveTableTarget(target: EventTarget | null): boolean {
  if (!hasClosest(target)) return false;
  return Boolean(target.closest(INTERACTIVE_TABLE_TARGET_SELECTOR));
}

export function shouldOpenTableRowFromClick(event: {
  defaultPrevented: boolean;
  target: EventTarget | null;
}): boolean {
  if (event.defaultPrevented) return false;
  return !isInteractiveTableTarget(event.target);
}

export function shouldOpenTableRowFromKey(event: {
  defaultPrevented: boolean;
  key: string;
  target: EventTarget | null;
}): boolean {
  if (event.defaultPrevented) return false;
  if (event.key !== 'Enter' && event.key !== ' ') return false;
  return !isInteractiveTableTarget(event.target);
}
