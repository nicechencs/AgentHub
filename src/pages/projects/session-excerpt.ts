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
  at?: string;
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
  at?: string;
};

export type PreviewTimelineItem =
  | { kind: 'convention' }
  | { kind: 'turn'; turn: ExcerptTurn }
  | { kind: 'approval'; decision: ApprovalDecision };

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
const TS_MARKER = /^---ts:(.+)---\s*$/;

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
  let pendingAt: string | undefined;
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
      turns.push(pendingAt ? { role, text: body, at: pendingAt } : { role, text: body });
      pendingAt = undefined;
    }
    kind = null;
    role = null;
  };
  for (const line of text.split('\n')) {
    const ts = line.match(TS_MARKER);
    if (ts) {
      pendingAt = ts[1]?.trim() || undefined;
      continue;
    }
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
    if (parsed) out.push(turn.at ? { ...parsed, at: parsed.at ?? turn.at } : parsed);
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

function parseTimeMs(value: string | undefined): number | null {
  if (!value?.trim()) return null;
  const ms = Date.parse(value);
  return Number.isFinite(ms) ? ms : null;
}

/** Convention first; approvals sit among turns by time, or after assistant replies if time is missing. */
export function buildPreviewTimeline(
  convention: string | null,
  turns: ExcerptTurn[],
  approvals: ApprovalDecision[],
): PreviewTimelineItem[] {
  const items: PreviewTimelineItem[] = [];
  if (convention?.trim()) items.push({ kind: 'convention' });
  if (approvals.length === 0) {
    for (const turn of turns) items.push({ kind: 'turn', turn });
    return items;
  }

  const turnTimes = turns.map((turn) => parseTimeMs(turn.at));
  const datedApprovals = approvals
    .map((decision, index) => ({ decision, index, at: parseTimeMs(decision.at) }))
    .sort((a, b) => {
      if (a.at != null && b.at != null && a.at !== b.at) return a.at - b.at;
      return a.index - b.index;
    });
  const canPlaceByTime = datedApprovals.some((item) => item.at != null) && turnTimes.some((ms) => ms != null);

  if (canPlaceByTime) {
    let next = 0;
    for (let i = 0; i < turns.length; i += 1) {
      const turnAt = turnTimes[i];
      while (
        next < datedApprovals.length
        && datedApprovals[next].at != null
        && turnAt != null
        && datedApprovals[next].at! <= turnAt
      ) {
        items.push({ kind: 'approval', decision: datedApprovals[next].decision });
        next += 1;
      }
      items.push({ kind: 'turn', turn: turns[i] });
    }
    while (next < datedApprovals.length) {
      items.push({ kind: 'approval', decision: datedApprovals[next].decision });
      next += 1;
    }
    return items;
  }

  const assistantSlots = turns
    .map((turn, index) => (turn.role === 'assistant' ? index : -1))
    .filter((index) => index >= 0);
  const after = new Map<number, ApprovalDecision[]>();
  datedApprovals.forEach((item, i) => {
    const slot =
      assistantSlots.length === 0
        ? turns.length - 1
        : assistantSlots[Math.min(i, assistantSlots.length - 1)];
    const list = after.get(slot) ?? [];
    list.push(item.decision);
    after.set(slot, list);
  });
  if (turns.length === 0) {
    for (const item of datedApprovals) items.push({ kind: 'approval', decision: item.decision });
    return items;
  }
  turns.forEach((turn, index) => {
    items.push({ kind: 'turn', turn });
    for (const decision of after.get(index) ?? []) {
      items.push({ kind: 'approval', decision });
    }
  });
  return items;
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
