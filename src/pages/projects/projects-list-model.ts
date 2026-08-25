import type { AgentProject, AgentSession } from '@/lib/types';
import { projectMatches, sessionMatches } from './project-filter';

/** Keep a parent when it matches, or when any already-loaded child matches the query. */
export function filterVisibleProjects(
  projects: AgentProject[],
  q: string,
  sessionsByProject: Record<string, AgentSession[]>,
): AgentProject[] {
  return projects.filter((p) => {
    if (projectMatches(p, q)) return true;
    if (!q) return true;
    const kids = sessionsByProject[p.id];
    if (!kids) return false;
    return kids.some((s) => sessionMatches(s, q));
  });
}

/** If the project itself matched, show all kids; else only matching kids. */
export function visibleSessionsForProject(
  projectId: string,
  projects: AgentProject[],
  q: string,
  sessionsByProject: Record<string, AgentSession[]>,
): AgentSession[] {
  const kids = sessionsByProject[projectId] ?? [];
  if (!q) return kids;
  const proj = projects.find((p) => p.id === projectId);
  if (proj && projectMatches(proj, q)) return kids;
  return kids.filter((s) => sessionMatches(s, q));
}

export function collectSelectableSessions(
  visibleProjects: AgentProject[],
  expanded: Set<string>,
  visibleSessionsFn: (projectId: string) => AgentSession[],
): AgentSession[] {
  const out: AgentSession[] = [];
  for (const p of visibleProjects) {
    if (!expanded.has(p.id)) continue;
    out.push(...visibleSessionsFn(p.id));
  }
  return out;
}

export function allVisibleSessionsSelected(
  selectableSessions: AgentSession[],
  selected: Set<string>,
): boolean {
  return selectableSessions.length > 0 && selectableSessions.every((s) => selected.has(s.id));
}

export function toggleSelectedSession(selected: Set<string>, id: string): Set<string> {
  const next = new Set(selected);
  if (next.has(id)) next.delete(id);
  else next.add(id);
  return next;
}

export function nextSelectedForToggleAllVisible(
  selected: Set<string>,
  selectableSessions: AgentSession[],
  allVisibleSelected: boolean,
): Set<string> {
  const next = new Set(selected);
  if (allVisibleSelected) {
    for (const s of selectableSessions) next.delete(s.id);
  } else {
    for (const s of selectableSessions) next.add(s.id);
  }
  return next;
}
