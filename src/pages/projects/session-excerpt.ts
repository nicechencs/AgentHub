/**
 * Visual turns from a project excerpt.
 * Core joins scanned pieces with a `---` line; a single blob is one turn.
 */
export function splitExcerptTurns(excerpt: string): string[] {
  const text = excerpt.replace(/\r\n/g, '\n').trim();
  if (!text) return [];
  return text
    .split(/^\s*---\s*$/m)
    .map((part) => part.trim())
    .filter(Boolean);
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
