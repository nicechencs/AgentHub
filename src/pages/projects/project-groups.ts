import { agentDisplayName } from '@/config/agents';
import { restoreProjectWorkspacePath } from '@/lib/path-open';
import type { AgentKey, AgentProject, AgentSession } from '@/lib/types';
import { displayTitle } from './project-format';

export type ProjectSortKey = 'time' | 'agent' | 'name';

export const PROJECT_SORT_KEYS: readonly ProjectSortKey[] = ['time', 'agent', 'name'];

export function parseProjectSortKey(raw: string | null | undefined): ProjectSortKey {
  return raw === 'agent' || raw === 'name' || raw === 'time' ? raw : 'time';
}

export type ProjectGroup = {
  /** Expand / hide key. A single member keeps that project id. */
  id: string;
  members: AgentProject[];
  primary: AgentProject;
  agentIds: AgentKey[];
  sessionCount: number;
  sizeBytes: number;
  updatedAt: string;
  hidden: boolean;
};

/** Windows filesystems are case-insensitive; POSIX paths are not. */
function isWindowsWorkspacePath(path: string): boolean {
  return /^[a-z]:[\\/]/i.test(path) || /^\\\\/.test(path);
}

/** Unify separators and trailing slashes without conflating POSIX case variants. */
export function normalizeProjectMergePath(path: string): string {
  const slashUnified = path.replace(/\\/g, '/');
  const normalized = slashUnified.replace(/\/+$/, '') || '/';
  return isWindowsWorkspacePath(path) ? normalized.toLowerCase() : normalized;
}

/**
 * Merge only on a restored workspace path. Storage / relative keys stay per project
 * so ungrouped buckets and agent-native dirs do not collapse together.
 */
export function projectMergeKey(p: AgentProject): string {
  const workspace = restoreProjectWorkspacePath(p);
  if (workspace) return `path:${normalizeProjectMergePath(workspace)}`;
  return `id:${p.id}`;
}

function compareText(a: string, b: string): number {
  return a.localeCompare(b, undefined, { sensitivity: 'base' });
}

function pickPrimary(members: AgentProject[]): AgentProject {
  return [...members].sort((a, b) => {
    const aPath = restoreProjectWorkspacePath(a) ? 1 : 0;
    const bPath = restoreProjectWorkspacePath(b) ? 1 : 0;
    if (aPath !== bPath) return bPath - aPath;
    return b.updatedAt.localeCompare(a.updatedAt);
  })[0];
}

export function toProjectGroup(members: AgentProject[]): ProjectGroup {
  const sortedMembers = [...members].sort((a, b) => a.agentId.localeCompare(b.agentId));
  const primary = pickPrimary(sortedMembers);
  const agentIds = [...new Set(sortedMembers.map((item) => item.agentId))];
  return {
    id: sortedMembers.length === 1 ? sortedMembers[0].id : projectMergeKey(primary),
    members: sortedMembers,
    primary,
    agentIds,
    sessionCount: sortedMembers.reduce((n, item) => n + item.sessionCount, 0),
    sizeBytes: sortedMembers.reduce((n, item) => n + item.sizeBytes, 0),
    updatedAt: sortedMembers.reduce(
      (latest, item) => (item.updatedAt > latest ? item.updatedAt : latest),
      sortedMembers[0].updatedAt,
    ),
    hidden: sortedMembers.every((item) => Boolean(item.hidden)),
  };
}

export function groupProjectsByPath(projects: AgentProject[], merge: boolean): ProjectGroup[] {
  if (!merge) return projects.map((project) => toProjectGroup([project]));
  const buckets = new Map<string, AgentProject[]>();
  for (const project of projects) {
    const key = projectMergeKey(project);
    const list = buckets.get(key);
    if (list) list.push(project);
    else buckets.set(key, [project]);
  }
  return [...buckets.values()].map(toProjectGroup);
}

export function sortProjectGroups(groups: ProjectGroup[], sort: ProjectSortKey): ProjectGroup[] {
  return [...groups].sort((a, b) => {
    if (sort === 'name') {
      return (
        compareText(displayTitle(a.primary), displayTitle(b.primary)) ||
        b.updatedAt.localeCompare(a.updatedAt)
      );
    }
    if (sort === 'agent') {
      const aName = a.agentIds.map((id) => agentDisplayName(id)).join('\0');
      const bName = b.agentIds.map((id) => agentDisplayName(id)).join('\0');
      return compareText(aName, bName) || b.updatedAt.localeCompare(a.updatedAt);
    }
    return (
      b.updatedAt.localeCompare(a.updatedAt) ||
      compareText(displayTitle(a.primary), displayTitle(b.primary))
    );
  });
}

export function sortSessions(sessions: AgentSession[], sort: ProjectSortKey): AgentSession[] {
  return [...sessions].sort((a, b) => {
    if (sort === 'name') {
      return compareText(a.title, b.title) || b.updatedAt.localeCompare(a.updatedAt);
    }
    if (sort === 'agent') {
      return (
        compareText(agentDisplayName(a.agentId), agentDisplayName(b.agentId)) ||
        b.updatedAt.localeCompare(a.updatedAt)
      );
    }
    return b.updatedAt.localeCompare(a.updatedAt) || compareText(a.title, b.title);
  });
}

export function groupCanExpand(group: ProjectGroup): boolean {
  return group.sessionCount > 0 || group.members.some((item) => item.agentId !== 'cursor');
}
