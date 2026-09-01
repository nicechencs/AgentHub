import { describe, expect, it } from 'vitest';
import {
  INTERACTIVE_TABLE_TARGET_SELECTOR,
  isInteractiveTableTarget,
  shouldOpenTableRowFromClick,
  shouldOpenTableRowFromKey,
} from './table-row-model';

function target(matches: boolean): EventTarget {
  return {
    closest: (selector: string) => {
      expect(selector).toBe(INTERACTIVE_TABLE_TARGET_SELECTOR);
      return matches ? {} : null;
    },
  } as unknown as EventTarget;
}

describe('table-row-model', () => {
  it('treats buttons, switches, and links as interactive', () => {
    expect(isInteractiveTableTarget(null)).toBe(false);
    expect(isInteractiveTableTarget(target(false))).toBe(false);
    expect(isInteractiveTableTarget(target(true))).toBe(true);
    expect(INTERACTIVE_TABLE_TARGET_SELECTOR).toContain('[role="switch"]');
  });

  it('opens from blank-row click, not from controls or prevented events', () => {
    expect(shouldOpenTableRowFromClick({ defaultPrevented: false, target: target(false) })).toBe(true);
    expect(shouldOpenTableRowFromClick({ defaultPrevented: false, target: target(true) })).toBe(false);
    expect(shouldOpenTableRowFromClick({ defaultPrevented: true, target: target(false) })).toBe(false);
  });

  it('opens from Enter/Space on the row, not from other keys', () => {
    expect(shouldOpenTableRowFromKey({
      defaultPrevented: false,
      key: 'Enter',
      target: target(false),
    })).toBe(true);
    expect(shouldOpenTableRowFromKey({
      defaultPrevented: false,
      key: ' ',
      target: target(false),
    })).toBe(true);
    expect(shouldOpenTableRowFromKey({
      defaultPrevented: false,
      key: 'Tab',
      target: target(false),
    })).toBe(false);
    expect(shouldOpenTableRowFromKey({
      defaultPrevented: false,
      key: 'Enter',
      target: target(true),
    })).toBe(false);
  });
});
