/**
 * Name-click lists open inspect from the name.
 * Row click (non-button) also opens detail when the pane is closed,
 * and switches the target while it is already expanded.
 */
export function followInspectOpen<T extends (...args: never[]) => unknown>(
  _expanded: boolean,
  open: T,
): T {
  return open;
}
