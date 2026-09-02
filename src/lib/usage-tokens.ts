import type { AgentKey, UsageRecord } from '@/lib/types';

/**
 * Token parts for metrics / distribution.
 *
 * Storage contract (aligned with ccusage after parse):
 * - `inputTokens` is always **billable / non-cached** input
 * - `cacheWriteTokens` is cache create (incl. 1h ephemeral write)
 * - `cacheReadTokens` is cache read / hit
 * - Codex / Grok: parser peels cache out of OpenAI-style full `input_tokens`
 *   once at ingest; UI must never peel again
 * - Claude / Kimi / Pi: Anthropic-style disjoint input + cache write/read
 *
 * The old `cache <= input ⇒ subtract` heuristic double-counted cache hits and
 * understated Codex input vs ccusage whenever cache ≤ half of the full prompt.
 */
export function usageTokenParts(
  r: Pick<UsageRecord, 'agentId' | 'inputTokens' | 'cacheReadTokens' | 'cacheWriteTokens'>,
): { billableInput: number; cacheRead: number; cacheWrite: number; cache: number; fullInput: number } {
  // agentId kept in the Pick so call sites stay uniform; layout no longer branches on it.
  const billableInput = Math.max(0, r.inputTokens);
  const cacheRead = Math.max(0, r.cacheReadTokens);
  const cacheWrite = Math.max(0, r.cacheWriteTokens);
  const cache = cacheRead + cacheWrite;
  return {
    billableInput,
    cacheRead,
    cacheWrite,
    cache,
    fullInput: billableInput + cache,
  };
}

export function sumBillableInput(
  rows: Array<
    Pick<UsageRecord, 'agentId' | 'inputTokens' | 'cacheReadTokens' | 'cacheWriteTokens'>
  >,
): number {
  return rows.reduce((s, r) => s + usageTokenParts(r).billableInput, 0);
}

export function isCodexAgent(id: AgentKey | string): boolean {
  return id === 'codex';
}
