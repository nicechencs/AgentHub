import type { AgentId, UsageRecord } from '@/lib/types';

/**
 * Token parts for metrics / distribution.
 *
 * Storage contract (aligned with ccusage after parse):
 * - `inputTokens` is always **billable / non-cached** input
 * - `cacheReadTokens` is a separate cache bucket
 * - Codex / Grok: parser peels cache out of OpenAI-style full `input_tokens`
 *   once at ingest; UI must never peel again
 * - Claude / Kimi / Pi: Anthropic-style disjoint input + cache
 *
 * The old `cache <= input ⇒ subtract` heuristic double-counted cache hits and
 * understated Codex input vs ccusage whenever cache ≤ half of the full prompt.
 */
export function usageTokenParts(
  r: Pick<UsageRecord, 'agentId' | 'inputTokens' | 'cacheReadTokens'>,
): { billableInput: number; cache: number; fullInput: number } {
  // agentId kept in the Pick so call sites stay uniform; layout no longer branches on it.
  const billableInput = Math.max(0, r.inputTokens);
  const cache = Math.max(0, r.cacheReadTokens);
  // full prompt size for cache-hit % (≈ billable + cache on ccusage input side)
  return {
    billableInput,
    cache,
    fullInput: billableInput + cache,
  };
}

export function sumBillableInput(
  rows: Array<Pick<UsageRecord, 'agentId' | 'inputTokens' | 'cacheReadTokens'>>,
): number {
  return rows.reduce((s, r) => s + usageTokenParts(r).billableInput, 0);
}

export function isCodexAgent(id: AgentId | string): boolean {
  return id === 'codex';
}
