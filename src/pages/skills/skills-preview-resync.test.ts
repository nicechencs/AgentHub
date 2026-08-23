import { describe, expect, it } from 'vitest';
import type { InstalledSkillDto } from '@/lib/api/skill';
import { previewTargetFromCatalogRow, visibleCatalogRows } from './SkillMatrix';
import {
  previewAfterHiddenAgent,
  previewAfterRemoveFromTool,
  resyncPreviewTarget,
} from './skills-preview-resync';

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

  it('keeps a shared-library preview on an agent projection after reload', () => {
    const shared = row({
      id: 'pdf',
      origin: 'shared',
      sourceDir: '/shared/pdf',
      projections: [
        { agent: 'claude', state: 'copied', targetDir: '/claude/pdf' },
        { agent: 'cursor', state: 'linked', targetDir: '/cursor/pdf', linkKind: 'junction' },
      ],
    });
    const preview = previewTargetFromCatalogRow(shared, 'cursor');
    const next = resyncPreviewTarget(preview, [shared]);
    expect(next).not.toBe('close');
    expect(next).not.toBe('keep');
    if (next === 'close' || next === 'keep') return;
    expect(next.includeShared).toBe(true);
    expect(next.privateAgent).toBe('cursor');
    expect(next.copies?.map((copy) => copy.agentId)).toEqual(['claude', 'cursor']);
  });

  it('falls back to the shared library tab when that agent projection is gone', () => {
    const before = row({
      id: 'pdf',
      origin: 'shared',
      sourceDir: '/shared/pdf',
      projections: [
        { agent: 'claude', state: 'copied', targetDir: '/claude/pdf' },
        { agent: 'cursor', state: 'linked', targetDir: '/cursor/pdf', linkKind: 'junction' },
      ],
    });
    const preview = previewTargetFromCatalogRow(before, 'cursor');
    const after = row({
      id: 'pdf',
      origin: 'shared',
      sourceDir: '/shared/pdf',
      projections: [{ agent: 'claude', state: 'copied', targetDir: '/claude/pdf' }],
    });
    const next = resyncPreviewTarget(preview, [after]);
    expect(next).not.toBe('close');
    expect(next).not.toBe('keep');
    if (next === 'close' || next === 'keep') return;
    expect(next.privateAgent).toBeNull();
    expect(next.sourceDir).toBe('/shared/pdf');
  });

  it('strips an in-flight mapped-copy delete and stays on the library tab', () => {
    const shared = row({
      id: 'pdf',
      origin: 'shared',
      sourceDir: '/shared/pdf',
      projections: [
        { agent: 'claude', state: 'copied', targetDir: '/claude/pdf' },
        { agent: 'cursor', state: 'linked', targetDir: '/cursor/pdf', linkKind: 'junction' },
      ],
    });
    const preview = previewTargetFromCatalogRow(shared, 'cursor');
    const next = resyncPreviewTarget(preview, [shared], {
      catalogReady: true,
      ignoreAgentId: 'cursor',
    });
    expect(next).not.toBe('close');
    expect(next).not.toBe('keep');
    if (next === 'close' || next === 'keep') return;
    expect(next.privateAgent).toBeNull();
    expect(next.sourceDir).toBe('/shared/pdf');
    expect(next.copies?.map((copy) => copy.agentId)).toEqual(['claude']);
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

describe('previewAfterRemoveFromTool', () => {
  const sharedPreview = () =>
    previewTargetFromCatalogRow(
      row({
        id: 'pdf',
        origin: 'shared',
        sourceDir: '/shared/pdf',
        projections: [
          { agent: 'claude', state: 'copied', targetDir: '/claude/pdf' },
          { agent: 'cursor', state: 'linked', targetDir: '/cursor/pdf', linkKind: 'junction' },
        ],
      }),
    );

  it('keeps the library tab when deleting a mapped copy with no siblings', () => {
    const preview = previewTargetFromCatalogRow(
      row({
        id: 'pdf',
        origin: 'shared',
        sourceDir: '/shared/pdf',
        projections: [{ agent: 'cursor', state: 'copied', targetDir: '/cursor/pdf' }],
      }),
      'cursor',
    );
    const next = previewAfterRemoveFromTool(preview, 'cursor');
    expect(next).not.toBe('close');
    if (next === 'close') return;
    expect(next.includeShared).toBe(true);
    expect(next.privateAgent).toBeNull();
    expect(next.sourceDir).toBe('/shared/pdf');
    expect(next.copies).toEqual([]);
  });

  it('does not jump off the library tab when other mapped copies remain', () => {
    const next = previewAfterRemoveFromTool(sharedPreview(), 'cursor');
    expect(next).not.toBe('close');
    if (next === 'close') return;
    expect(next.privateAgent).toBeNull();
    expect(next.sourceDir).toBe('/shared/pdf');
    expect(next.copies?.map((copy) => copy.agentId)).toEqual(['claude']);
  });

  it('closes a private-only preview when the last copy is removed', () => {
    const preview = previewTargetFromCatalogRow(
      row({
        id: 'pet',
        origin: 'codex',
        mapStatus: 'private_source',
        sourceDir: '/codex/pet',
      }),
      'codex',
    );
    expect(previewAfterRemoveFromTool(preview, 'codex')).toBe('close');
  });
});

describe('previewAfterHiddenAgent', () => {
  it('returns an includeShared preview to the library when the selected tool is hidden', () => {
    const preview = previewTargetFromCatalogRow(
      row({
        id: 'pdf',
        origin: 'shared',
        sourceDir: '/shared/pdf',
        projections: [{ agent: 'cursor', state: 'copied', targetDir: '/cursor/pdf' }],
      }),
      'cursor',
    );
    const next = previewAfterHiddenAgent(preview, new Set());
    expect(next).not.toBe('close');
    expect(next).not.toBe('keep');
    if (next === 'close' || next === 'keep') return;
    expect(next.privateAgent).toBeNull();
    expect(next.sourceDir).toBe('/shared/pdf');
  });

  it('closes a private-only preview when no visible copy remains', () => {
    const preview = previewTargetFromCatalogRow(
      row({
        id: 'pet',
        origin: 'codex',
        mapStatus: 'private_source',
        sourceDir: '/codex/pet',
      }),
      'codex',
    );
    expect(previewAfterHiddenAgent(preview, new Set())).toBe('close');
  });
});
