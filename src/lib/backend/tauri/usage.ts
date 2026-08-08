import type { UsageCollectResult, UsagePort } from '@/lib/backend/contracts';
import {
  mapCoreParserHealth,
  mapCoreUsageRecord,
  type CoreParserHealth,
  type CoreUsageRecord,
} from '@/lib/backend/contracts/usage-map';
import type { UsageAvailability } from '@/lib/backend/contracts/usage-types';
import type { ParserHealth, UsageRecord, UsageTrendPoint } from '@/lib/types';
import { invoke } from './invoke';

/**
 * Production Usage — wired to core UsageService via Tauri commands.
 */
export function createTauriUsagePort(): UsagePort {
  return {
    async getAvailability(): Promise<UsageAvailability> {
      const v = await invoke<{ status: string; reason?: string }>('usage_get_availability');
      if (v.status === 'available') return { status: 'available' };
      return {
        status: 'unavailable',
        reason: v.reason ?? 'Usage backend 不可用',
      };
    },

    async queryUsage(q): Promise<UsageRecord[]> {
      const agentId = !q.agentId || q.agentId === 'all' ? null : q.agentId;
      const model = !q.model || q.model === 'all' ? null : q.model;
      const rows = await invoke<CoreUsageRecord[]>('usage_query', {
        days: q.days,
        agentId,
        model,
      });
      return rows.map(mapCoreUsageRecord);
    },

    async usageTrend(days, agentId): Promise<UsageTrendPoint[]> {
      const id = !agentId || agentId === 'all' ? null : agentId;
      return invoke<UsageTrendPoint[]>('usage_trend', {
        days,
        agentId: id,
      });
    },

    async listModels(): Promise<string[]> {
      return invoke<string[]>('usage_list_models');
    },

    async parserHealth(): Promise<ParserHealth[]> {
      const rows = await invoke<CoreParserHealth[]>('usage_parser_health');
      return rows.map(mapCoreParserHealth);
    },

    async missingPricingModels(days = 30): Promise<string[]> {
      return invoke<string[]>('usage_missing_pricing', { days });
    },

    async collectUsage(onProgress): Promise<UsageCollectResult> {
      onProgress?.(10);
      const result = await invoke<UsageCollectResult>('usage_collect', { agentId: null });
      onProgress?.(100);
      return result;
    },
  };
}
