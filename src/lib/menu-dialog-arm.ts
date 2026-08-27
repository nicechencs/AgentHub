/**
 * Menu → Dialog open helpers: prevent dismiss-on-open for Radix menus.
 * Shared by Connections ticket add menu and Agents card actions.
 */

/** After the originating click, so menu unmount cannot dismiss the new dialog. */
export function scheduleAfterMenuClose(action: () => void, delayMs = 0): void {
  const schedule = globalThis.setTimeout;
  if (typeof schedule === 'function') {
    schedule(action, delayMs);
    return;
  }
  action();
}

/** Swallow a leftover Dialog `onOpenChange(false)` from the opening click. */
export function shouldIgnoreMenuDialogDismiss(armed: boolean, nextOpen: boolean): boolean {
  return armed && !nextOpen;
}

/** Connections `openTicketAdd` and AgentCard uninstall: clear the arm after the click settles. */
export const MENU_DIALOG_DISMISS_CLEAR_MS = 100;

type MenuDialogSchedule = (fn: () => void, delayMs?: number) => void;

/** Arm ignore-dismiss, open the dialog, then clear the arm after `delayMs`. */
export function armMenuDialogOpen(
  arm: { current: boolean },
  open: () => void,
  delayMs = MENU_DIALOG_DISMISS_CLEAR_MS,
  schedule: MenuDialogSchedule = scheduleAfterMenuClose,
): void {
  arm.current = true;
  open();
  schedule(() => {
    arm.current = false;
  }, delayMs);
}

/**
 * Menu item that opens a Dialog: preventDefault keeps the menu mounted through
 * the click; arm + delayed clear swallows the leftover dismiss.
 */
export function handleMenuDialogSelect(
  event: { preventDefault: () => void },
  arm: { current: boolean },
  open: () => void,
  delayMs = MENU_DIALOG_DISMISS_CLEAR_MS,
  schedule: MenuDialogSchedule = scheduleAfterMenuClose,
): void {
  event.preventDefault();
  armMenuDialogOpen(arm, open, delayMs, schedule);
}
