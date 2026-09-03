/**
 * Visual turns from a project excerpt.
 *
 * Core now emits role-tagged blocks (`---turn:user---` / `---turn:assistant---`)
 * so markdown horizontal rules inside a reply are not treated as turn splits.
 * Legacy excerpts still join pieces with a `---` line; even index is user.
 */

export type ExcerptTurnRole = 'user' | 'assistant';

export type ExcerptTurn = {
  role: ExcerptTurnRole;
  text: string;
};

export function excerptTurnsToRecordLines(
  turns: ExcerptTurn[],
  labels: { user: string; assistant: string },
): { speaker: string; text: string }[] {
  return turns.map((turn) => ({
    speaker: turn.role === 'user' ? labels.user : labels.assistant,
    text: turn.text,
  }));
}

const ROLE_MARKER = /^---turn:(user|assistant)---\s*$/;

export function splitExcerptTurns(excerpt: string): ExcerptTurn[] {
  const text = excerpt.replace(/\r\n/g, '\n').trim();
  if (!text) return [];
  if (hasRoleMarkers(text)) {
    return splitRoleTaggedTurns(text);
  }
  return text
    .split(/^\s*---\s*$/m)
    .map((part) => part.trim())
    .filter(Boolean)
    .map((part, index) => ({
      role: (index % 2 === 0 ? 'user' : 'assistant') as ExcerptTurnRole,
      text: part,
    }));
}

function hasRoleMarkers(text: string): boolean {
  return text.split('\n').some((line) => ROLE_MARKER.test(line));
}

function splitRoleTaggedTurns(text: string): ExcerptTurn[] {
  const turns: ExcerptTurn[] = [];
  let role: ExcerptTurnRole | null = null;
  const buf: string[] = [];
  const flush = () => {
    if (!role) {
      buf.length = 0;
      return;
    }
    const body = buf.join('\n').trim();
    buf.length = 0;
    if (body) turns.push({ role, text: body });
  };
  for (const line of text.split('\n')) {
    const marked = line.match(ROLE_MARKER);
    if (marked) {
      flush();
      role = marked[1] as ExcerptTurnRole;
      continue;
    }
    buf.push(line);
  }
  flush();
  return turns;
}

export type ExcerptLoadResult =
  | { status: 'ready'; excerpt: string }
  | { status: 'empty' }
  | { status: 'error' };

/** Core skips failed ids instead of rejecting; missing row is an error, blank body is empty. */
export function classifyExcerptRows(
  sessionId: string,
  rows: { id: string; excerpt?: string | null }[],
): ExcerptLoadResult {
  const row = rows.find((r) => r.id === sessionId);
  if (!row) return { status: 'error' };
  const text = row.excerpt?.trim() ?? '';
  if (!text) return { status: 'empty' };
  return { status: 'ready', excerpt: text };
}
