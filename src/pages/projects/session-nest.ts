import type { AgentSession } from '@/lib/types';

const SUBAGENT_RE = /(?:^|\/)agent-transcripts\/([^/]+)\/subagents\//i;
const TRANSCRIPT_RE = /(?:^|\/)agent-transcripts\/([^/]+)/i;

function slashPath(s: string): string {
  return s.replace(/\\/g, '/');
}

/** Parent transcript id when this row is a Cursor subagent file. */
export function cursorSubagentParentId(s: Pick<AgentSession, 'relativePath' | 'path'>): string | null {
  const rel = slashPath(s.relativePath || s.path || '');
  const m = rel.match(SUBAGENT_RE);
  return m?.[1] ?? null;
}

export function cursorTranscriptId(
  s: Pick<AgentSession, 'sessionId' | 'relativePath' | 'path'>,
): string | null {
  const sid = s.sessionId?.trim();
  if (sid && !cursorSubagentParentId(s)) return sid;
  const rel = slashPath(s.relativePath || s.path || '');
  const m = rel.match(TRANSCRIPT_RE);
  return m?.[1] ?? sid ?? null;
}

export type NestedSession = {
  session: AgentSession;
  children: AgentSession[];
};

/** Hang Cursor subagent rows under their parent transcript. Other agents stay flat. */
export function nestSessions(sessions: AgentSession[]): NestedSession[] {
  const childrenByParent = new Map<string, AgentSession[]>();
  const nestedIds = new Set<string>();
  for (const s of sessions) {
    const parentId = cursorSubagentParentId(s);
    if (!parentId) continue;
    nestedIds.add(s.id);
    const list = childrenByParent.get(parentId) ?? [];
    list.push(s);
    childrenByParent.set(parentId, list);
  }

  const out: NestedSession[] = [];
  const usedParents = new Set<string>();
  for (const s of sessions) {
    if (nestedIds.has(s.id)) continue;
    const tid = cursorTranscriptId(s);
    const children = (tid && childrenByParent.get(tid)) || [];
    if (tid) usedParents.add(tid);
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
