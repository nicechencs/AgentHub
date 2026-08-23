import { describe, expect, it } from 'vitest';
import type { InstalledSkillDto } from '@/lib/api/skill';
import { previewTargetFromCatalogRow, visibleCatalogRows } from './SkillMatrix';
import { resyncPreviewTarget } from './skills-preview-resync';

function row(
  partial: Partial<InstalledSkillDto> & Pick<InstalledSkillDto, 'id' | 'origin'>,
): InstalledSkillDto {
  return {
    name: partial.id,
    description: '',
    sourceDir: `/${partial.origin}/${partial.id}`,
    rootLabel: '',
    rootDir: `/${partial.origin}`,
    projectable: partial.origin === 'shared',
    mapStatus: partial.origin === 'shared' ? 'available' : 'private_source',
    source: null,
    projections: [],
    ...partial,
  };
}

describe('resyncPreviewTarget', () => {
  it('keeps a private preview open when content hash changes', () => {
    const before = visibleCatalogRows([
      row({
        id: 'pet',
        origin: 'cursor',
        mapStatus: 'private_source',
        contentHash: 'old',
      }),
    ])[0]!;
    const preview = previewTargetFromCatalogRow(before, 'cursor');
    const after = visibleCatalogRows([
      row({
        id: 'pet',
        origin: 'cursor',
        mapStatus: 'private_source',
        contentHash: 'new',
        sourceDir: '/cursor/pet',
      }),
    ]);
    const next = resyncPreviewTarget(preview, after);
    expect(next).not.toBe('close');
    expect(next).not.toBe('keep');
    if (next === 'close' || next === 'keep') return;
    expect(next.privateAgent).toBe('cursor');
    expect(next.rowKey).toBe('private:pet:new');
  });

  it('stays on the edited agent when a merged group splits', () => {
    const merged = visibleCatalogRows([
      row({
        id: 'pet',
        origin: 'claude',
        mapStatus: 'private_source',
        contentHash: 'same',
        sourceDir: '/claude/pet',
      }),
      row({
        id: 'pet',
        origin: 'cursor',
        mapStatus: 'private_source',
        contentHash: 'same',
        sourceDir: '/cursor/pet',
      }),
    ])[0]!;
    const preview = previewTargetFromCatalogRow(merged, 'cursor');
    const split = visibleCatalogRows([
      row({
        id: 'pet',
        origin: 'claude',
        mapStatus: 'private_source',
        contentHash: 'a',
        sourceDir: '/claude/pet',
      }),
      row({
        id: 'pet',
        origin: 'cursor',
        mapStatus: 'private_source',
        contentHash: 'b',
        sourceDir: '/cursor/pet',
      }),
    ]);
    const next = resyncPreviewTarget(preview, split);
    expect(next).not.toBe('close');
    expect(next).not.toBe('keep');
    if (next === 'close' || next === 'keep') return;
    expect(next.privateAgent).toBe('cursor');
    expect(next.copies?.map((copy) => copy.agentId)).toEqual(['cursor']);
    expect(next.rowKey).toBe('private:pet:b');
  });

  it('strips an in-flight delete from stale catalog copies', () => {
    const merged = visibleCatalogRows([
      row({
        id: 'pet',
        origin: 'claude',
        mapStatus: 'private_source',
        contentHash: 'same',
        sourceDir: '/claude/pet',
      }),
      row({
        id: 'pet',
        origin: 'cursor',
        mapStatus: 'private_source',
        contentHash: 'same',
        sourceDir: '/cursor/pet',
      }),
    ])[0]!;
    const preview = previewTargetFromCatalogRow(merged, 'claude');
    const next = resyncPreviewTarget(preview, [merged], {
      catalogReady: true,
      ignoreAgentId: 'cursor',
    });
    expect(next).not.toBe('close');
    expect(next).not.toBe('keep');
    if (next === 'close' || next === 'keep') return;
    expect(next.copies?.map((copy) => copy.agentId)).toEqual(['claude']);
    expect(next.privateAgent).toBe('claude');
  });

  it('falls back to the shared row after adopt', () => {
    const priv = visibleCatalogRows([
      row({
        id: 'pet',
        origin: 'codex',
        mapStatus: 'private_source',
        contentHash: 'x',
      }),
    ])[0]!;
    const preview = previewTargetFromCatalogRow(priv, 'codex');
    const next = resyncPreviewTarget(preview, [
      row({ id: 'pet', origin: 'shared', rootLabel: '~/.agents/skills' }),
    ]);
    expect(next).not.toBe('close');
    expect(next).not.toBe('keep');
    if (next === 'close' || next === 'keep') return;
    expect(next.privateAgent).toBeNull();
    expect(next.rowKey).toBe('shared:pet');
  });

  it('does not close while the catalog has not loaded', () => {
    const priv = visibleCatalogRows([
      row({
        id: 'pet',
        origin: 'codex',
        mapStatus: 'private_source',
        contentHash: 'x',
      }),
    ])[0]!;
    const preview = previewTargetFromCatalogRow(priv, 'codex');
    expect(resyncPreviewTarget(preview, [], { catalogReady: false })).toBe('keep');
  });
});
