import type { AgentId } from '@/lib/types';

export interface UsageQuery {
  /** 回看天数（后端按 now - days 过滤；UI 可映射 today/24h → 1） */
  days: number;
  agentId?: AgentId | 'all';
  model?: string | 'all';
}

/**
 * Usage 接入状态。
 * - available：后端已接线，空数组表示真实零数据
 * - unavailable：尚未接线 / 不可用，UI 不得展示假零数据
 */
export type UsageAvailability =
  | { status: 'available' }
  | { status: 'unavailable'; reason: string };

/** Collect pass result (camelCase; mirrors core CollectResult). */
export interface UsageCollectResultDto {
  inserted: number;
  skipped: number;
  failed: number;
  missingPricingModels?: string[];
}
