import type { InstalledSkillDto } from '@/lib/api/skill';
import type { SkillPreviewTarget } from './SkillMarkdownPreviewPanel';
import {
  isPrivateSourceRow,
  isSharedCatalogRow,
  previewTargetFromCatalogRow,
  privateRowCopies,
} from './SkillMatrix';

export function previewTargetsEqual(a: SkillPreviewTarget, b: SkillPreviewTarget): boolean {
  const copiesKey = (target: SkillPreviewTarget) =>
    (target.copies ?? []).map((copy) => `${copy.agentId}:${copy.sourceDir}`).join('|');
  return (
    a.rowKey === b.rowKey &&
    a.privateAgent === b.privateAgent &&
    a.sourceDir === b.sourceDir &&
    copiesKey(a) === copiesKey(b)
  );
}

function stripIgnoredCopy(
  target: SkillPreviewTarget,
  ignoreAgentId?: string | null,
): SkillPreviewTarget {
  if (!ignoreAgentId || !target.copies?.length) return target;
  const copies = target.copies.filter((copy) => copy.agentId !== ignoreAgentId);
  if (copies.length === target.copies.length) return target;
  if (copies.length === 0) return target;
  const selected =
    copies.find((copy) => copy.agentId === target.privateAgent) ?? copies[0]!;
  return {
    ...target,
    copies,
    privateAgent: selected.agentId,
    sourceDir: selected.sourceDir,
  };
}

/**
 * Re-bind an open preview to catalog rows by physical copy (skill id + agent),
 * not by content hash. Hash stays in the matrix row key so split groups remain
 * distinct; an FS-watch reload after editing SKILL.md must not close the pane.
 */
export function resyncPreviewTarget(
  preview: SkillPreviewTarget,
  localRows: InstalledSkillDto[],
  options: { catalogReady: boolean; ignoreAgentId?: string | null } = {
    catalogReady: true,
  },
): SkillPreviewTarget | 'keep' | 'close' {
  const { catalogReady, ignoreAgentId } = options;
  if (!catalogReady && localRows.length === 0) return 'keep';

  const skillId = preview.skillId;
  const agent = preview.privateAgent ?? null;

  if (agent) {
    const withCopy = localRows.find((item) => {
      if (item.id !== skillId || !isPrivateSourceRow(item)) return false;
      return privateRowCopies(item).some(
        (copy) => copy.agentId === agent && copy.agentId !== ignoreAgentId,
      );
    });
    if (withCopy) {
      return stripIgnoredCopy(
        previewTargetFromCatalogRow(withCopy, agent),
        ignoreAgentId,
      );
    }
    const shared = localRows.find(
      (item) => isSharedCatalogRow(item) && item.id === skillId,
    );
    if (shared) return previewTargetFromCatalogRow(shared);
    return catalogReady ? 'close' : 'keep';
  }

  const shared = localRows.find(
    (item) => isSharedCatalogRow(item) && item.id === skillId,
  );
  if (shared) return previewTargetFromCatalogRow(shared);
  return catalogReady ? 'close' : 'keep';
}
