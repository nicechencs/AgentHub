/**
 * Skill markdown preview façade + mock port contract.
 * Production code lives in skill.ts / backend ports; this file is test-only.
 */
import { beforeEach, describe, expect, it } from 'vitest';
import { resetBackend } from '@/app/runtime';
import { readSkillMarkdown } from '@/lib/api/skill';

describe('readSkillMarkdown (browser mock)', () => {
  beforeEach(() => {
    resetBackend();
  });

  it('returns markdown body for a shared library skill', async () => {
    const preview = await readSkillMarkdown('notes');
    expect(preview.skillId).toBe('notes');
    expect(preview.name).toBeTruthy();
    expect(preview.content).toContain('#');
    expect(preview.content).toMatch(/notes|何时使用|步骤/);
    expect(preview.path.toLowerCase()).toContain('skill.md');
    expect(preview.truncated).toBe(false);
  });

  it('returns markdown body for a private agent skill', async () => {
    const preview = await readSkillMarkdown('sample-pet', 'codex');
    expect(preview.skillId).toBe('sample-pet');
    expect(preview.name).toBe('sample-pet');
    expect(preview.content).toContain('sample-pet');
    expect(preview.content).toContain('origin');
    expect(preview.truncated).toBe(false);
  });

  it('rejects unknown skill ids', async () => {
    await expect(readSkillMarkdown('definitely-not-a-skill-xyz')).rejects.toThrow(
      /技能不存在|not found|definitely-not-a-skill-xyz/i,
    );
  });

  it('preview DTO always exposes the GUI-facing camelCase fields', async () => {
    const preview = await readSkillMarkdown('pdf');
    expect(Object.keys(preview).sort()).toEqual(
      ['content', 'name', 'path', 'skillId', 'truncated'].sort(),
    );
    expect(typeof preview.content).toBe('string');
    expect(typeof preview.name).toBe('string');
    expect(typeof preview.path).toBe('string');
    expect(typeof preview.skillId).toBe('string');
    expect(typeof preview.truncated).toBe('boolean');
  });
});
