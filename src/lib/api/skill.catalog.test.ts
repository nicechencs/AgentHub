/**
 * Skill catalog façade + mock port contract.
 * Production code lives in skill.ts / backend ports; this file is test-only.
 */
import { beforeEach, describe, expect, it } from 'vitest';
import { resetBackend } from '@/app/runtime';
import { listInstalledSkills, listSkillCatalog } from '@/lib/api/skill';
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

  it('omits claude copies of dbs-action (available) and pdf (conflict)', async () => {
    const catalog = await listSkillCatalog();
    expect(
      catalog.some((row) => row.origin === 'claude' && row.id === 'dbs-action'),
    ).toBe(false);
    expect(catalog.some((row) => row.origin === 'claude' && row.id === 'pdf')).toBe(
      false,
    );
    expect(catalog.some((row) => row.origin === 'shared' && row.id === 'dbs-action')).toBe(
      true,
    );
    expect(catalog.some((row) => row.origin === 'shared' && row.id === 'pdf')).toBe(true);
  });

  it('includes private-only rows such as hatch-pet (codex)', async () => {
    const catalog = await listSkillCatalog();
    const hatch = catalog.find((row) => row.id === 'hatch-pet');
    expect(hatch).toMatchObject({
      id: 'hatch-pet',
      origin: 'codex',
      mapStatus: 'private_source',
      projectable: false,
      projections: [],
    });
    expect(catalog.some((row) => row.id === 'changelog-generator')).toBe(true);
    expect(catalog.some((row) => row.id === 'local-review')).toBe(true);
    expect(catalog.some((row) => row.id === 'grok-session-notes')).toBe(true);
  });

  it('does not change listInstalledSkills semantics (still includes available/conflict copies)', async () => {
    const installed = await listInstalledSkills();
    expect(
      installed.some((row) => row.origin === 'claude' && row.id === 'dbs-action'),
    ).toBe(true);
    expect(installed.some((row) => row.origin === 'claude' && row.id === 'pdf')).toBe(
      true,
    );
    expect(installed.some((row) => row.origin === 'codex' && row.id === 'hatch-pet')).toBe(
      true,
    );
  });

  it('is implemented on both SkillPort adapters', () => {
    expect(typeof createMockSkillPort().listSkillCatalog).toBe('function');
    expect(typeof createTauriSkillPort().listSkillCatalog).toBe('function');
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
