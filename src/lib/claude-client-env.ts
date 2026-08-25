/**
 * Claude Code client helpers for the `[1m]` marker and window choice.
 * Declared token counts come from route/provider config, not a model catalog.
 */

export const CLAUDE_WINDOW_1M = 1_048_576;
export const CLAUDE_WINDOW_200K = 200_000;

export const CLAUDE_CONTEXT_WINDOW_OPTIONS = [
  { value: 'auto', tokens: null },
  { value: '200000', tokens: CLAUDE_WINDOW_200K },
  { value: '1048576', tokens: CLAUDE_WINDOW_1M },
] as const;

export type ClaudeContextWindowChoice = (typeof CLAUDE_CONTEXT_WINDOW_OPTIONS)[number]['value'];

export function stripClaudeContextMarker(model: string): string {
  const trimmed = model.trim();
  for (const suffix of ['[1m]', '[1M]']) {
    if (trimmed.endsWith(suffix)) return trimmed.slice(0, -suffix.length).trimEnd();
  }
  return trimmed;
}

export function claudeContextWindowFor(
  model: string,
  overrideTokens?: number | null,
): number | null {
  if (overrideTokens && overrideTokens > 0) return overrideTokens;
  const trimmed = model.trim();
  if (trimmed.endsWith('[1m]') || trimmed.endsWith('[1M]')) return CLAUDE_WINDOW_1M;
  return null;
}

export function formatClaudeContextWindow(tokens: number | null | undefined): string {
  if (!tokens) return '';
  if (tokens === CLAUDE_WINDOW_1M) return '1M';
  if (tokens === CLAUDE_WINDOW_200K) return '200k';
  if (tokens % 1000 === 0) return `${tokens / 1000}k`;
  return String(tokens);
}

export function parseContextWindowChoice(raw: string | undefined): ClaudeContextWindowChoice {
  const trimmed = (raw ?? '').trim();
  if (!trimmed || trimmed === 'auto') return 'auto';
  if (trimmed === '1048576' || trimmed === '1000000') return '1048576';
  if (trimmed === '200000') return '200000';
  if (Number(trimmed) === CLAUDE_WINDOW_1M) return '1048576';
  return 'auto';
}

export function contextWindowTokensFromChoice(
  choice: ClaudeContextWindowChoice,
): number | null {
  if (choice === 'auto') return null;
  const parsed = Number(choice);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : null;
}
