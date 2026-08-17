import { describe, expect, it } from 'vitest';
import { resolveAutoInstallPlan, runtimeChannelForPlan } from './env-plan';
import type { RuntimeDetect } from '@/lib/types';

function missing(id: RuntimeDetect['id']): RuntimeDetect {
  return { id, status: 'missing', remediations: [] };
}

describe('runtimeChannelForPlan', () => {
  it('uses brew on macOS', () => {
    expect(runtimeChannelForPlan('macos')).toBe('brew');
  });

  it('uses winget on Windows and manual remediations on Linux/unknown', () => {
    expect(runtimeChannelForPlan('windows')).toBe('winget');
    expect(runtimeChannelForPlan('linux')).toBe('manual');
    expect(runtimeChannelForPlan('unknown')).toBe('manual');
  });
});

describe('resolveAutoInstallPlan', () => {
  it('skips one-click targets on Linux', () => {
    const plan = resolveAutoInstallPlan([missing('nodejs'), missing('git')], undefined, 'linux');
    expect(plan.targets).toEqual([]);
    expect(plan.skipped).toEqual(['nodejs', 'git']);
  });

  it('keeps one-click Node/Git on Windows', () => {
    const plan = resolveAutoInstallPlan([missing('nodejs'), missing('git')], undefined, 'windows');
    expect(plan.targets).toEqual(['nodejs', 'git']);
    expect(plan.skipped).toEqual([]);
  });
});
