import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { AgentKey } from '@/lib/types';
import { createMockSkillPort, resetMockSkills } from './skill';

describe('mock SkillPort.onFsChanged', () => {
  it('is a no-op and never invokes the handler', async () => {
    const port = createMockSkillPort();
    const handler = vi.fn();
    const unsub = await port.onFsChanged(handler);
    expect(typeof unsub).toBe('function');
    expect(handler).not.toHaveBeenCalled();
    unsub();
    expect(handler).not.toHaveBeenCalled();
  });
});

describe('resetMockSkills', () => {
  beforeEach(() => {
    resetMockSkills();
  });
  afterEach(() => {
    resetMockSkills();
  });

  it('restores the seeded catalog after mutations', async () => {
    const port = createMockSkillPort();
    const originalSkills = await port.listSkills();
    const originalInstalled = await port.listInstalledSkills();
    const originalIds = originalSkills.map((s) => s.id);
    const originalPrivateKeys = originalInstalled
      .filter((s) => s.origin !== 'shared')
      .map((s) => `${s.origin}:${s.id}`);

    await port.uninstallSkill(originalIds[0]);
    await port.installMarketSkill('brand-new-market-skill');
    await port.uninstallSkill('sample-changelog', 'claude');

    const mutated = await port.listSkills();
    expect(mutated.some((s) => s.id === originalIds[0])).toBe(false);
    expect(mutated.some((s) => s.id === 'brand-new-market-skill')).toBe(true);

    resetMockSkills();

    const restored = await port.listSkills();
    expect(restored.map((s) => s.id)).toEqual(originalIds);
    expect(restored[0]?.sync).toEqual(originalSkills[0]?.sync);
    expect(restored[0]?.projections).toEqual(originalSkills[0]?.projections);

    const restoredInstalled = await port.listInstalledSkills();
    expect(
      restoredInstalled.filter((s) => s.origin !== 'shared').map((s) => `${s.origin}:${s.id}`),
    ).toEqual(originalPrivateKeys);
    expect(
      restoredInstalled.some((s) => s.origin === 'shared' && s.id === originalIds[0]),
    ).toBe(true);
  }, 15_000);

  it('clears projection-mode memory', async () => {
    const port = createMockSkillPort();
    const skills = await port.listSkills();
    let hit: { skillId: string; agentId: AgentKey } | undefined;
    for (const skill of skills) {
      for (const [agentId, state] of Object.entries(skill.sync)) {
        if (state === 'absent') {
          hit = { skillId: skill.id, agentId: agentId as AgentKey };
          break;
        }
      }
      if (hit) break;
    }
    expect(hit).toBeDefined();
    const { skillId, agentId } = hit!;

    await port.toggleSkillSync(skillId, agentId, { mode: 'link' });
    await port.toggleSkillSync(skillId, agentId);
    expect((await port.toggleSkillSync(skillId, agentId)).state).toBe('linked');

    resetMockSkills();

    const after = (await port.listSkills()).find((s) => s.id === skillId);
    expect(after?.sync[agentId]).toBe('absent');
    expect((await port.toggleSkillSync(skillId, agentId)).state).toBe('copied');
  }, 15_000);
});
