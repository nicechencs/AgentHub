/**
 * Name-click lists open inspect from the name.
 * Row click (non-button) also opens detail when the pane is closed,
 * and switches the target while it is already expanded.
 */
export function followInspectOpen<Args extends unknown[], Result>(
  _expanded: boolean,
  open: (...args: Args) => Result,
): (...args: Args) => Result {
  return open;
}
