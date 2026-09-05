import { agentDisplayName } from '@/config/agents';
import type { TranslateFn } from '@/lib/i18n';
import type { AgentSession } from '@/lib/types';
import type { ProjectSortKey } from './project-groups';

export const REVIEW_THREAD_KIND = 'review';

export function isReviewSession(s: Pick<AgentSession, 'threadKind'>): boolean {
  return s.threadKind === REVIEW_THREAD_KIND;
}

/** Codex spawned child (not a tool-approval record). */
export function isSpawnedChildSession(
  s: Pick<AgentSession, 'parentSessionId' | 'threadKind'>,
): boolean {
  return Boolean(s.parentSessionId?.trim()) && !isReviewSession(s);
}

/** Hang Codex children under the root conversation id. */
export function spawnedChildParentKey(
  s: Pick<AgentSession, 'sessionId' | 'parentSessionId' | 'threadKind'>,
): string | null {
  if (!isSpawnedChildSession(s)) return null;
  return s.sessionId?.trim() || s.parentSessionId?.trim() || null;
}

export function nestedSessionLabel(
  session: Pick<AgentSession, 'agentRole'>,
  t: TranslateFn,
): string {
  switch (session.agentRole?.trim().toLowerCase()) {
    case 'explorer':
      return t('projects.tree.subSessionExplore');
    case 'coder':
    case 'implementer':
      return t('projects.tree.subSessionCode');
    case 'reviewer':
    case 'review':
      return t('projects.tree.subSessionCheck');
    case 'planner':
      return t('projects.tree.subSessionPlan');
    default:
      return t('projects.tree.subSession');
  }
}

/** Codex tool-approval threads that belong to this conversation. */
export function reviewsForParent(
  parent: Pick<AgentSession, 'sessionId' | 'parentSessionId' | 'agentId'>,
  sessions: AgentSession[],
): AgentSession[] {
  if (parent.parentSessionId?.trim()) return [];
  const sid = parent.sessionId?.trim();
  if (!sid) return [];
  return sessions.filter(
    (s) =>
      isReviewSession(s) &&
      s.parentSessionId === sid &&
      s.agentId === parent.agentId,
  );
}

/** Cursor / Claude: `…/<parentId>/subagents/…` */
const SUBAGENT_DIR_RE = /(?:^|\/)([^/]+)\/subagents\//i;
/** Kimi: `session_<uuid>/agents/<name>/` other than `main`. */
const KIMI_AGENT_RE = /(?:^|\/)session_([^/]+)\/agents\/(?!main(?:\/|$))/i;
const CURSOR_TRANSCRIPT_RE = /(?:^|\/)agent-transcripts\/([^/]+)/i;

function slashPath(s: string): string {
  return s.replace(/\\/g, '/');
}

/** Parent transcript id when this row is a nested subagent file. */
export function cursorSubagentParentId(s: Pick<AgentSession, 'relativePath' | 'path'>): string | null {
  const rel = slashPath(s.relativePath || s.path || '');
  const kimi = rel.match(KIMI_AGENT_RE);
  if (kimi?.[1]) return kimi[1];
  const sub = rel.match(SUBAGENT_DIR_RE);
  return sub?.[1] ?? null;
}

export function cursorTranscriptId(
  s: Pick<AgentSession, 'sessionId' | 'relativePath' | 'path'>,
): string | null {
  const sid = s.sessionId?.trim();
  if (sid && !cursorSubagentParentId(s)) {
    const slash = sid.indexOf('/');
    return slash > 0 ? sid.slice(0, slash) : sid;
  }
  const rel = slashPath(s.relativePath || s.path || '');
  const m = rel.match(CURSOR_TRANSCRIPT_RE);
  return m?.[1] ?? (sid || null);
}

export type NestedSession = {
  session: AgentSession;
  children: AgentSession[];
};

/** All-tab mixes agents; do not let a Cursor UUID claim a Codex spawn child. */
function nestParentKey(agentId: AgentSession['agentId'] | undefined, key: string): string {
  return `${agentId ?? ''}::${key}`;
}

function compareText(a: string, b: string): number {
  return a.localeCompare(b, undefined, { sensitivity: 'base' });
}

/** Newest activity in a nested group (parent or any child). */
export function latestNestedActivity(session: AgentSession, children: AgentSession[]): string {
  let latest = session.updatedAt;
  for (const child of children) {
    if (child.updatedAt > latest) latest = child.updatedAt;
  }
  return latest;
}

/** Hang subagent rows under their parent transcript. Other rows stay flat. */
export function nestSessions(sessions: AgentSession[]): NestedSession[] {
  const listed = sessions.filter((s) => !isReviewSession(s));
  const childrenByParent = new Map<string, AgentSession[]>();
  const nestedIds = new Set<string>();
  const pushChild = (parentId: string, s: AgentSession) => {
    nestedIds.add(s.id);
    const list = childrenByParent.get(parentId) ?? [];
    list.push(s);
    childrenByParent.set(parentId, list);
  };
  for (const s of listed) {
    const parentId = cursorSubagentParentId(s);
    if (parentId) pushChild(nestParentKey(s.agentId, parentId), s);
  }
  for (const s of listed) {
    if (nestedIds.has(s.id)) continue;
    const parentId = spawnedChildParentKey(s);
    if (parentId) pushChild(nestParentKey(s.agentId, parentId), s);
  }

  const out: NestedSession[] = [];
  const usedParents = new Set<string>();
  for (const s of listed) {
    if (nestedIds.has(s.id)) continue;
    const keys = [cursorTranscriptId(s), s.sessionId?.trim()]
      .filter(
        (key, index, all): key is string => Boolean(key) && all.indexOf(key) === index,
      )
      .map((key) => nestParentKey(s.agentId, key));
    const children: AgentSession[] = [];
    const seen = new Set<string>();
    for (const key of keys) {
      usedParents.add(key);
      for (const child of childrenByParent.get(key) ?? []) {
        if (seen.has(child.id)) continue;
        seen.add(child.id);
        children.push(child);
      }
    }
    out.push({ session: s, children });
  }
  for (const [key, children] of childrenByParent) {
    if (usedParents.has(key)) continue;
    for (const child of children) {
      out.push({ session: child, children: [] });
    }
  }
  return out;
}

/** One list pipeline: nest, then sort. Time uses the newest child so All-tab page 1 keeps active trees. */
export function nestedSessionRows(
  sessions: AgentSession[],
  sort: ProjectSortKey,
): NestedSession[] {
  return sortNestedSessions(nestSessions(sessions), sort);
}

/** Sort parent rows. Time uses the newest child so All-tab pagination keeps active trees on page 1. */
export function sortNestedSessions(
  rows: NestedSession[],
  sort: ProjectSortKey,
): NestedSession[] {
  return [...rows].sort((a, b) => {
    if (sort === 'name') {
      return (
        compareText(a.session.title, b.session.title) ||
        b.session.updatedAt.localeCompare(a.session.updatedAt)
      );
    }
    if (sort === 'agent') {
      return (
        compareText(agentDisplayName(a.session.agentId), agentDisplayName(b.session.agentId)) ||
        b.session.updatedAt.localeCompare(a.session.updatedAt)
      );
    }
    const aTime = latestNestedActivity(a.session, a.children);
    const bTime = latestNestedActivity(b.session, b.children);
    return bTime.localeCompare(aTime) || compareText(a.session.title, b.session.title);
  });
}

/** Flat list in nest-friendly order: parents sit at the group's latest activity. */
export function sessionsOrderedForNesting(
  sessions: AgentSession[],
  sort: ProjectSortKey,
): AgentSession[] {
  const nested = nestedSessionRows(sessions, sort);
  const out: AgentSession[] = [];
  for (const { session, children } of nested) {
    out.push(session, ...children);
  }
  return out;
}

/** Rows shown in the tree: parents always, children only when that parent is open. */
export function flattenVisibleSessions(
  sessions: AgentSession[],
  nestedOpen: Set<string>,
): AgentSession[] {
  const out: AgentSession[] = [];
  for (const { session, children } of nestSessions(sessions)) {
    out.push(session);
    if (children.length > 0 && nestedOpen.has(session.id)) {
      out.push(...children);
    }
  }
  return out;
}
