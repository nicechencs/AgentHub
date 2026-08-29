import { verifiedProjectWorkspacePath } from '@/lib/path-open';
import type { AgentProject } from '@/lib/types';
import type { InstalledSkillDto } from '@/lib/api/skill';

export type ProjectSkillOption = {
  /** Absolute workspace path (dropdown value). */
  workspacePath: string;
  /** Alias or title from the project list. */
  label: string;
  /** Normalized path shown as secondary text. */
  subtitle: string;
};

function workspaceKey(path: string): string {
  return path.replace(/\\/g, '/').replace(/\/+$/, '').toLowerCase();
}

/** Unique workspaces from the project list that have an openable path. */
export function projectSkillOptions(projects: AgentProject[]): ProjectSkillOption[] {
  const byPath = new Map<string, ProjectSkillOption>();
  for (const project of projects) {
    const path = verifiedProjectWorkspacePath(project);
    if (!path) continue;
    const key = workspaceKey(path);
    if (byPath.has(key)) continue;
    const label = (project.alias?.trim() || project.title || path).trim() || path;
    byPath.set(key, { workspacePath: path, label, subtitle: path });
  }
  return [...byPath.values()].sort((a, b) =>
    a.label.localeCompare(b.label, undefined, { sensitivity: 'base' }),
  );
}

export function matchProjectSkillOption(
  options: ProjectSkillOption[],
  workspacePath: string | null | undefined,
): ProjectSkillOption | null {
  if (!workspacePath) return null;
  const key = workspaceKey(workspacePath);
  return options.find((option) => workspaceKey(option.workspacePath) === key) ?? null;
}

export function filterProjectSkillRows(
  rows: InstalledSkillDto[],
  search: string,
): InstalledSkillDto[] {
  const q = search.trim().toLowerCase();
  if (!q) return rows;
  return rows.filter((row) => {
    const hay = `${row.name} ${row.id} ${row.description} ${row.rootLabel}`.toLowerCase();
    return hay.includes(q);
  });
}

export function projectSkillRowKey(row: Pick<InstalledSkillDto, 'origin' | 'id'>): string {
  return `project:${row.origin}:${row.id}`;
}
