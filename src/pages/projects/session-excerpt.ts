/**
 * Visual turns from a project excerpt.
 *
 * Core now emits role-tagged blocks (`---turn:user---` / `---turn:assistant---`)
 * so markdown horizontal rules inside a reply are not treated as turn splits.
 * Optional `---doc:convention---` holds project instructions pulled out of the
 * first user turn. Legacy excerpts still join pieces with a `---` line; even
 * index is user.
 */

export type ExcerptTurnRole = 'user' | 'assistant';

export type ExcerptTurn = {
  role: ExcerptTurnRole;
  text: string;
};

export type ExcerptDocument = {
  convention: string | null;
  turns: ExcerptTurn[];
};

export type ApprovalDecision = {
  outcome: string;
  rationale: string;
  riskLevel?: string;
  raw: string;
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

const TURN_MARKER = /^---turn:(user|assistant)---\s*$/;
const DOC_MARKER = /^---doc:([a-z]+)---\s*$/;

export function splitExcerptTurns(excerpt: string): ExcerptTurn[] {
  return splitExcerptDocument(excerpt).turns;
}

export function splitExcerptDocument(excerpt: string): ExcerptDocument {
  const text = excerpt.replace(/\r\n/g, '\n').trim();
  if (!text) return { convention: null, turns: [] };
  if (hasBlockMarkers(text)) {
    return splitTaggedDocument(text);
  }
  return {
    convention: null,
    turns: text
      .split(/^\s*---\s*$/m)
      .map((part) => part.trim())
      .filter(Boolean)
      .map((part, index) => ({
        role: (index % 2 === 0 ? 'user' : 'assistant') as ExcerptTurnRole,
        text: part,
      })),
  };
}

function hasBlockMarkers(text: string): boolean {
  return text.split('\n').some((line) => TURN_MARKER.test(line) || DOC_MARKER.test(line));
}

function splitTaggedDocument(text: string): ExcerptDocument {
  const turns: ExcerptTurn[] = [];
  let convention: string | null = null;
  let kind: 'turn' | 'doc' | null = null;
  let role: ExcerptTurnRole | null = null;
  const buf: string[] = [];
  const flush = () => {
    const body = buf.join('\n').trim();
    buf.length = 0;
    if (!body) {
      kind = null;
      role = null;
      return;
    }
    if (kind === 'doc') {
      if (!convention) convention = body;
    } else if (kind === 'turn' && role) {
      turns.push({ role, text: body });
    }
    kind = null;
    role = null;
  };
  for (const line of text.split('\n')) {
    const turn = line.match(TURN_MARKER);
    if (turn) {
      flush();
      kind = 'turn';
      role = turn[1] as ExcerptTurnRole;
      continue;
    }
    const doc = line.match(DOC_MARKER);
    if (doc) {
      flush();
      kind = 'doc';
      role = null;
      continue;
    }
    buf.push(line);
  }
  flush();
  return { convention, turns };
}

export function parseApprovalDecisions(turns: ExcerptTurn[]): ApprovalDecision[] {
  const out: ApprovalDecision[] = [];
  for (const turn of turns) {
    if (turn.role !== 'assistant') continue;
    const parsed = parseApprovalJson(turn.text);
    if (parsed) out.push(parsed);
  }
  return out;
}

function parseApprovalJson(text: string): ApprovalDecision | null {
  const raw = text.trim();
  if (!raw.startsWith('{') || !raw.includes('outcome')) return null;
  try {
    const value = JSON.parse(raw) as Record<string, unknown>;
    if (typeof value.outcome !== 'string' || !value.outcome.trim()) return null;
    return {
      outcome: value.outcome.trim(),
      rationale: typeof value.rationale === 'string' ? value.rationale.trim() : '',
      riskLevel: typeof value.risk_level === 'string' ? value.risk_level : undefined,
      raw,
    };
  } catch {
    return null;
  }
}

export type ExcerptLoadResult =
  | { status: 'ready'; excerpt: string; truncated: boolean }
  | { status: 'empty' }
  | { status: 'error' };

/** Core skips failed ids instead of rejecting; missing row is an error, blank body is empty. */
export function classifyExcerptRows(
  sessionId: string,
  rows: { id: string; excerpt?: string | null; truncated?: boolean | null }[],
): ExcerptLoadResult {
  const row = rows.find((r) => r.id === sessionId);
  if (!row) return { status: 'error' };
  const text = row.excerpt?.trim() ?? '';
  if (!text) return { status: 'empty' };
  return { status: 'ready', excerpt: text, truncated: row.truncated === true };
}
