import { describe, expect, it } from 'vitest';
import { sumBillableInput, usageTokenParts } from './usage-tokens';

describe('usageTokenParts (ccusage layout)', () => {
  it('never peels cache from codex stored non-cached input', () => {
    // Parser stored billable=750 from full=1000, cache=250 (ccusage inputTokens=750).
    // Old heuristic cache<=input would wrongly yield 500.
    const p = usageTokenParts({
      agentId: 'codex',
      inputTokens: 750,
      cacheReadTokens: 250,
    });
    expect(p.billableInput).toBe(750);
    expect(p.cache).toBe(250);
    expect(p.fullInput).toBe(1000);
  });

  it('is stable across repeated application (no erosion)', () => {
    let billable = 750;
    const cache = 250;
    for (let i = 0; i < 5; i++) {
      const p = usageTokenParts({
        agentId: 'codex',
        inputTokens: billable,
        cacheReadTokens: cache,
      });
      expect(p.billableInput).toBe(750);
      billable = p.billableInput;
    }
  });

  it('never peels cache from grok stored non-cached input', () => {
    const p = usageTokenParts({
      agentId: 'grok',
      inputTokens: 7180,
      cacheReadTokens: 11264,
    });
    expect(p.billableInput).toBe(7180);
    expect(p.cache).toBe(11264);
    expect(p.fullInput).toBe(7180 + 11264);
  });

  it('keeps anthropic-style disjoint buckets', () => {
    const p = usageTokenParts({
      agentId: 'claude',
      inputTokens: 100,
      cacheReadTokens: 50,
    });
    expect(p.billableInput).toBe(100);
    expect(p.cache).toBe(50);
    expect(p.fullInput).toBe(150);
  });

  it('sums billable input without double-subtract', () => {
    const total = sumBillableInput([
      { agentId: 'codex', inputTokens: 750, cacheReadTokens: 250 },
      { agentId: 'claude', inputTokens: 100, cacheReadTokens: 10 },
      { agentId: 'grok', inputTokens: 7180, cacheReadTokens: 11264 },
    ]);
    expect(total).toBe(750 + 100 + 7180);
  });

  it('clamps negative inputs to zero', () => {
    const p = usageTokenParts({
      agentId: 'codex',
      inputTokens: -5,
      cacheReadTokens: -1,
    });
    expect(p.billableInput).toBe(0);
    expect(p.cache).toBe(0);
    expect(p.fullInput).toBe(0);
  });

  it('handles high-cache codex rows without peel', () => {
    // After normalize: billable 10642, cache 11008 (cache > billable is normal).
    const p = usageTokenParts({
      agentId: 'codex',
      inputTokens: 10642,
      cacheReadTokens: 11008,
    });
    expect(p.billableInput).toBe(10642);
    expect(p.cache).toBe(11008);
    expect(p.fullInput).toBe(10642 + 11008);
  });
});
