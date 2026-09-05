/**
 * Name-click lists open inspect from the name.
 * While the pane is already expanded, empty-row selection may switch the target.
 * A closed pane must stay closed.
 */
export function followInspectOpen<T extends (...args: never[]) => unknown>(
  expanded: boolean,
  open: T,
): T | undefined {
  return expanded ? open : undefined;
}
