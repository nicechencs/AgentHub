import type { AgentKey } from '@/lib/types';

/** Trend series grouping. Default `agent` keeps `{ date, claude?: n, ... }`. */
export type UsageTrendGroupBy = 'agent' | 'model';

export interface UsageQuery {
  /** 回看天数（后端按 now - days 过滤；UI 可映射 today/24h → 1） */
  days: number;
  agentId?: AgentKey | 'all';
  model?: string | 'all';
  /** Soft cap on raw rows (`ORDER BY ts DESC`). Dashboard table uses 2000. */
  limit?: number;
  /** RFC3339 lower bound, AND-ed with the days window. */
  since?: string;
  /** RFC3339 exclusive upper bound (`ts < until`). Custom range uses next local midnight. */
  until?: string;
  /** Hidden agents; applied before LIMIT so the table cap is among visible rows. */
  excludeAgentIds?: AgentKey[];
}

/** SQL-aggregate totals. `billableInput` = stored input; full prompt = billable + write + read. */
export interface UsageOverviewMetrics {
  billableInput: number;
  output: number;
  cacheRead: number;
  cacheWrite: number;
  costUsd: number;
}

/**
 * 契约不变量：distribution 必须覆盖查询窗口内的全部行（含未知 agent / 零 token 行）。
 * 前端 filterHiddenUsageOverview 依赖 distribution 切片重算 metrics；
 * 若后端排除任何切片，隐藏过滤后的指标会静默漏算。
 */
export interface UsageOverviewDistributionSlice {
  key: string;
  tokens: number;
  costUsd: number;
  billableInput: number;
  output: number;
  cacheRead: number;
  cacheWrite: number;
}

export interface UsageOverview {
  metrics: UsageOverviewMetrics;
  distribution: UsageOverviewDistributionSlice[];
  models: string[];
}

/**
 * Usage 接入状态。
 * - available：后端已接线，空数组表示真实零数据
 * - unavailable：尚未接线 / 不可用，UI 不得展示假零数据
 */
export type UsageAvailability =
  | { status: 'available' }
  | { status: 'unavailable'; reason: string };

/** Downstream conversation surface observed by the local gateway. */
export type GatewayUsageSurface = 'messages' | 'responses' | 'chat';

/** One per-request row captured by the local gateway (bridge). */
export interface GatewayUsageRow {
  requestId: string;
  ts: string;
  profileId: string;
  surface: string;
  upstreamChannel?: string;
  ticketId?: string;
  accountSourceKind?: string;
  accountSourceId?: string;
  model?: string;
  upstreamModel?: string;
  inputTokens: number;
  outputTokens: number;
  cachedInputTokens?: number;
  reasoningTokens?: number;
  status: 'ok' | 'failed' | string;
  statusCode?: number;
  errorClass?: string;
  latencyMs?: number;
  ttftMs?: number;
  attempts?: number;
  sessionId?: string;
}

/** Filter for local gateway usage queries (time range optional). */
export interface GatewayUsageQuery {
  since?: string | null;
  until?: string | null;
  profileId?: string | null;
  limit?: number | null;
}

/** Aggregated local gateway usage overview for a time window. */
export interface GatewayUsageOverview {
  requestCount: number;
  okCount: number;
  failedCount: number;
  inputTokens: number;
  outputTokens: number;
  cachedInputTokens: number;
  reasoningTokens: number;
  avgLatencyMs?: number;
  p95LatencyMs?: number;
  avgTtftMs?: number;
}
