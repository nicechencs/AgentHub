import { describe, expect, it } from 'vitest';
import { createExclusiveActionGate, createLatestRequestGate } from './latest-request';

describe('plugin inventory request generations', () => {
  it('accepts only the newest request and can invalidate on unmount', () => {
    const gate = createLatestRequestGate();
    const first = gate.begin();
    const second = gate.begin();
    expect(gate.isCurrent(first)).toBe(false);
    expect(gate.isCurrent(second)).toBe(true);
    gate.invalidate();
    expect(gate.isCurrent(second)).toBe(false);
  });

  it('allows only one mutation until the active action settles', () => {
    const gate = createExclusiveActionGate();
    expect(gate.begin()).toBe(true);
    expect(gate.begin()).toBe(false);
    gate.end();
    expect(gate.begin()).toBe(true);
  });
});
