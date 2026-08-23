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
  if (target.includeShared) {
    const kept =
      target.privateAgent && target.privateAgent !== ignoreAgentId
        ? copies.find((copy) => copy.agentId === target.privateAgent)
        : undefined;
    return {
      ...target,
      copies,
      privateAgent: kept?.agentId ?? null,
      sourceDir: kept?.sourceDir ?? target.libraryDir ?? target.sourceDir,
    };
  }
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

/** Optimistic preview after deleting one tool folder. Shared library stays open. */
export function previewAfterRemoveFromTool(
  preview: SkillPreviewTarget,
  agentId: string,
): SkillPreviewTarget | 'close' {
  const remaining = (preview.copies ?? []).filter((copy) => copy.agentId !== agentId);
  if (preview.includeShared) {
    const leaveAgent = preview.privateAgent == null || preview.privateAgent === agentId;
    const kept = remaining.find((copy) => copy.agentId === preview.privateAgent);
    return {
      ...preview,
      copies: remaining,
      privateAgent: leaveAgent ? null : (kept?.agentId ?? null),
      sourceDir: leaveAgent
        ? (preview.libraryDir ?? preview.sourceDir)
        : (kept?.sourceDir ?? preview.libraryDir ?? preview.sourceDir),
    };
  }
  if (remaining.length === 0) return 'close';
  const next =
    remaining.find((copy) => copy.agentId === preview.privateAgent) ?? remaining[0]!;
  return {
    ...preview,
    copies: remaining,
    privateAgent: next.agentId,
    sourceDir: next.sourceDir,
  };
}

/**
 * When a previewed agent is hidden or uninstalled: jump to another visible
 * copy, or to the shared library tab. Private-only previews close if none remain.
 */
export function previewAfterHiddenAgent(
  preview: SkillPreviewTarget,
  visibleAgentIds: ReadonlySet<string>,
): SkillPreviewTarget | 'keep' | 'close' {
  const origin = preview.privateAgent;
  if (!origin || visibleAgentIds.has(origin)) return 'keep';
  const next = (preview.copies ?? []).find((copy) => visibleAgentIds.has(copy.agentId));
  if (next) {
    return { ...preview, privateAgent: next.agentId, sourceDir: next.sourceDir };
  }
  if (preview.includeShared) {
    return {
      ...preview,
      privateAgent: null,
      sourceDir: preview.libraryDir ?? preview.sourceDir,
    };
  }
  return 'close';
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
    if (shared) {
      return stripIgnoredCopy(
        previewTargetFromCatalogRow(shared, agent),
        ignoreAgentId,
      );
    }
    return catalogReady ? 'close' : 'keep';
  }

  const shared = localRows.find(
    (item) => isSharedCatalogRow(item) && item.id === skillId,
  );
  if (shared) {
    return stripIgnoredCopy(previewTargetFromCatalogRow(shared), ignoreAgentId);
  }
  return catalogReady ? 'close' : 'keep';
}
