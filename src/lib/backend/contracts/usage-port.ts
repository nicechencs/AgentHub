import type { AgentId, ParserHealth, UsageRecord, UsageTrendPoint } from '@/lib/types';
import type { UsageAvailability, UsageQuery } from './usage-types';

/** Result of a usage collect pass (mirrors core CollectResult). */
export interface UsageCollectResult {
  inserted: number;
  skipped: number;
  failed: number;
  agents: Array<{
    agentId: AgentId;
    supported: boolean;
    records: number;
    failRatePct?: number;
    skipped?: number;
  }>;
  missingPricingModels?: string[];
}

export interface UsagePort {
  /** 明确区分「已接入零数据」与「尚未接入」 */
  getAvailability(): Promise<UsageAvailability>;
  queryUsage(q: UsageQuery): Promise<UsageRecord[]>;
  usageTrend(days: number, agentId?: AgentId | 'all'): Promise<UsageTrendPoint[]>;
  listModels(): Promise<string[]>;
  parserHealth(): Promise<ParserHealth[]>;
  /** Models lacking embedded pricing in recent usage_records */
  missingPricingModels?(days?: number): Promise<string[]>;
  collectUsage(onProgress?: (pct: number) => void): Promise<UsageCollectResult | void>;
}
