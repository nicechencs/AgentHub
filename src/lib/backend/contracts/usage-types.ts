import type { AgentId } from '@/lib/types';

export interface UsageQuery {
  /** 回看天数（后端按 now - days 过滤；UI 可映射 today/24h → 1） */
  days: number;
  agentId?: AgentId | 'all';
  model?: string | 'all';
  /** Soft cap on raw rows (`ORDER BY ts DESC`). Dashboard table uses 2000. */
  limit?: number;
  /** RFC3339 lower bound, AND-ed with the days window. */
  since?: string;
  /** Hidden agents; applied before LIMIT so the table cap is among visible rows. */
  excludeAgentIds?: AgentId[];
}

/** SQL-aggregate totals. `billableInput` = stored input; full prompt = billable + cache. */
export interface UsageOverviewMetrics {
  billableInput: number;
  output: number;
  cache: number;
  costUsd: number;
}

export interface UsageOverviewDistributionSlice {
  key: string;
  tokens: number;
  costUsd: number;
  billableInput: number;
  output: number;
  cache: number;
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
