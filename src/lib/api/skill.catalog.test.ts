/**
 * Skill catalog façade + mock port contract.
 * Production code lives in skill.ts / backend ports; this file is test-only.
 */
import { beforeEach, describe, expect, it } from 'vitest';
import { resetBackend } from '@/app/runtime';
import { listInstalledSkills, listProjectSkills, listSkillCatalog } from '@/lib/api/skill';
import { createTauriSkillPort } from '@/lib/backend/tauri/skill';
import { createMockSkillPort } from '@/dev/mocks/skill';
import {
  invalidateSkills,
  useSkillCatalog,
  useSkillsCacheVersion,
  type SkillsCacheKey,
} from '@/lib/hooks/useSkills';

describe('listSkillCatalog (browser mock)', () => {
  beforeEach(() => {
    resetBackend();
  });

  it('marks every shared row origin=shared, projectable, with projections', async () => {
    const catalog = await listSkillCatalog();
    const shared = catalog.filter((row) => row.origin === 'shared');
    expect(shared.length).toBeGreaterThan(0);
    for (const row of shared) {
      expect(row.origin).toBe('shared');
      expect(row.projectable).toBe(true);
      expect(row.mapStatus).toBe('available');
      expect(row.projections.length).toBeGreaterThan(0);
    }
  });

  it('keeps only private_source agent rows with empty projections', async () => {
    const catalog = await listSkillCatalog();
    const privateRows = catalog.filter((row) => row.origin !== 'shared');
    expect(privateRows.length).toBeGreaterThan(0);
    for (const row of privateRows) {
      expect(row.origin).not.toBe('shared');
      expect(row.mapStatus).toBe('private_source');
      expect(row.projections).toEqual([]);
    }
  });

  it('omits claude copies of notes (available) and pdf (conflict)', async () => {
    const catalog = await listSkillCatalog();
    expect(
      catalog.some((row) => row.origin === 'claude' && row.id === 'notes'),
    ).toBe(false);
    expect(catalog.some((row) => row.origin === 'claude' && row.id === 'pdf')).toBe(
      false,
    );
    expect(catalog.some((row) => row.origin === 'shared' && row.id === 'notes')).toBe(
      true,
    );
    expect(catalog.some((row) => row.origin === 'shared' && row.id === 'pdf')).toBe(true);
  });

  it('includes private-only rows such as sample-pet (codex)', async () => {
    const catalog = await listSkillCatalog();
    const pet = catalog.filter((row) => row.id === 'sample-pet');
    expect(pet).toHaveLength(2);
    expect(pet.map((row) => row.origin).sort()).toEqual(['codex', 'cursor']);
    expect(pet[0]?.contentHash).toBe(pet[1]?.contentHash);
    expect(pet[0]).toMatchObject({
      id: 'sample-pet',
      mapStatus: 'private_source',
      projectable: false,
      projections: [],
    });
    expect(catalog.some((row) => row.id === 'sample-changelog')).toBe(true);
    expect(catalog.some((row) => row.id === 'sample-review')).toBe(true);
    expect(catalog.some((row) => row.id === 'sample-notes')).toBe(true);
  });

  it('does not change listInstalledSkills semantics (still includes available/conflict copies)', async () => {
    const installed = await listInstalledSkills();
    expect(
      installed.some((row) => row.origin === 'claude' && row.id === 'notes'),
    ).toBe(true);
    expect(installed.some((row) => row.origin === 'claude' && row.id === 'pdf')).toBe(
      true,
    );
    expect(installed.some((row) => row.origin === 'codex' && row.id === 'sample-pet')).toBe(
      true,
    );
  });

  it('lists seeded project skills for a known workspace', async () => {
    const rows = await listProjectSkills('C:\\Users\\demo\\app');
    expect(rows.some((row) => row.id === 'demo-notes')).toBe(true);
    expect(rows.every((row) => row.projectable === false)).toBe(true);
    expect(rows.every((row) => row.origin.startsWith('.'))).toBe(true);
  });

  it('is implemented on both SkillPort adapters', () => {
    expect(typeof createMockSkillPort().listSkillCatalog).toBe('function');
    expect(typeof createTauriSkillPort().listSkillCatalog).toBe('function');
    expect(typeof createMockSkillPort().listProjectSkills).toBe('function');
    expect(typeof createTauriSkillPort().listProjectSkills).toBe('function');
    expect(typeof createMockSkillPort().installProjectSkill).toBe('function');
    expect(typeof createTauriSkillPort().uninstallProjectSkill).toBe('function');
  });
});

describe('SkillsCacheKey catalog invalidation', () => {
  it('replaces installed with catalog on invalidateSkills / hook exports', () => {
    type HasInstalled = 'installed' extends SkillsCacheKey ? true : false;
    type HasCatalog = 'catalog' extends SkillsCacheKey ? true : false;
    const installedGone: HasInstalled = false;
    const catalogPresent: HasCatalog = true;
    expect(installedGone).toBe(false);
    expect(catalogPresent).toBe(true);
    expect(typeof useSkillsCacheVersion).toBe('function');
    expect(typeof useSkillCatalog).toBe('function');
    invalidateSkills('catalog');
    invalidateSkills(['skills', 'catalog', 'market']);
  });
});
