import { describe, expect, it, vi } from 'vitest';
import {
  closeConfirmationOnOpenChange,
  preventBusyConfirmationDismissal,
} from './busy-confirmation';

describe('closeConfirmationOnOpenChange', () => {
  it('does not close while busy even if Radix reports open=false', () => {
    const onClose = vi.fn();
    closeConfirmationOnOpenChange(false, true, onClose);
    expect(onClose).not.toHaveBeenCalled();
  });

  it('closes when dismissed and not busy', () => {
    const onClose = vi.fn();
    closeConfirmationOnOpenChange(false, false, onClose);
    expect(onClose).toHaveBeenCalledOnce();
  });

  it('does not close when the dialog stays open', () => {
    const onClose = vi.fn();
    closeConfirmationOnOpenChange(true, false, onClose);
    closeConfirmationOnOpenChange(true, true, onClose);
    expect(onClose).not.toHaveBeenCalled();
  });
});

describe('preventBusyConfirmationDismissal', () => {
  it('calls preventDefault while busy', () => {
    const preventDefault = vi.fn();
    preventBusyConfirmationDismissal(true, { preventDefault });
    expect(preventDefault).toHaveBeenCalledOnce();
  });

  it('does not preventDefault when idle', () => {
    const preventDefault = vi.fn();
    preventBusyConfirmationDismissal(false, { preventDefault });
    expect(preventDefault).not.toHaveBeenCalled();
  });
});
