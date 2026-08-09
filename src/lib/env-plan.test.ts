import { describe, expect, it } from 'vitest';
import { runtimeChannelForPlan } from './env-plan';

describe('runtimeChannelForPlan', () => {
  it('uses brew on macOS', () => {
    expect(runtimeChannelForPlan('macos')).toBe('brew');
  });

  it('uses winget on Windows and fallback hosts', () => {
    expect(runtimeChannelForPlan('windows')).toBe('winget');
    expect(runtimeChannelForPlan('unknown')).toBe('winget');
  });
});
