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
