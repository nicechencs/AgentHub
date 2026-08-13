/** A controlled confirmation dialog must stay visible while its mutation is in flight. */
export function closeConfirmationOnOpenChange(open: boolean, busy: boolean, onClose: () => void): void {
  if (!open && !busy) onClose();
}

/** Radix dismissal events need an explicit preventDefault while a mutation is in flight. */
export function preventBusyConfirmationDismissal(busy: boolean, event: Pick<Event, 'preventDefault'>): void {
  if (busy) event.preventDefault();
}
