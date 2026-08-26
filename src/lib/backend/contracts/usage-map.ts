import type { AgentId, ParserHealth, UsageRecord } from '@/lib/types';

/** Core UsageRecord (Rust camelCase). */
export interface CoreUsageRecord {
  id: string;
  agentId: AgentId;
  accountId?: string | null;
  model: string;
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheWriteTokens: number;
  costUsd?: number | null;
  sessionId?: string | null;
  ts: string;
  rawHash?: string | null;
}

export interface CoreParserHealth {
  agentId: AgentId;
  supported: boolean;
  records: number;
  failRatePct?: number | null;
  skipped?: number | null;
}

export function mapCoreUsageRecord(r: CoreUsageRecord): UsageRecord {
  return {
    id: r.id,
    timestamp: r.ts,
    agentId: r.agentId,
    model: r.model ?? 'unknown',
    inputTokens: r.inputTokens ?? 0,
    outputTokens: r.outputTokens ?? 0,
    cacheReadTokens: r.cacheReadTokens ?? 0,
    cacheWriteTokens: r.cacheWriteTokens ?? 0,
    costUsd: r.costUsd ?? 0,
    sessionId: r.sessionId ?? '',
  };
}

export function mapCoreParserHealth(h: CoreParserHealth): ParserHealth {
  return {
    agentId: h.agentId,
    supported: h.supported,
    records: h.records,
    failRatePct: h.failRatePct ?? undefined,
    skipped: h.skipped ?? undefined,
  };
}
