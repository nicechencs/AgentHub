/**
 * Usage API façade — production via Tauri UsageService; mock under dev:mock.
 */
import { getBackend } from '@/app/runtime';
import type { UsageCollectResult } from '@/lib/backend/contracts/ports';
import type { AgentKey, ParserHealth, UsageRecord, UsageTrendPoint } from '@/lib/types';

export type {
  GatewayUsageOverview,
  GatewayUsageQuery,
  GatewayUsageRow,
  UsageAvailability,
  UsageOverview,
  UsageOverviewDistributionSlice,
  UsageOverviewMetrics,
  UsageQuery,
  UsageTrendGroupBy,
} from '@/lib/backend/contracts/usage-types';
export type { UsageCollectResult };
import type { GatewayUsageOverview, GatewayUsageQuery, GatewayUsageRow, UsageAvailability, UsageOverview, UsageQuery, UsageTrendGroupBy } from '@/lib/backend/contracts/usage-types';

export async function getUsageAvailability(): Promise<UsageAvailability> {
  return getBackend().usage.getAvailability();
}

export async function queryUsage(q: UsageQuery): Promise<UsageRecord[]> {
  return getBackend().usage.queryUsage(q);
}

export async function usageOverview(q: UsageQuery): Promise<UsageOverview> {
  return getBackend().usage.usageOverview(q);
}

export async function usageTrend(
  days: number,
  agentId?: AgentKey | 'all',
  model?: string,
  since?: string,
  excludeAgentIds?: AgentKey[],
  groupBy?: UsageTrendGroupBy,
): Promise<UsageTrendPoint[]> {
  return getBackend().usage.usageTrend(days, agentId, model, since, excludeAgentIds, groupBy);
}

export async function listModels(): Promise<string[]> {
  return getBackend().usage.listModels();
}

export async function parserHealth(): Promise<ParserHealth[]> {
  return getBackend().usage.parserHealth();
}

export async function missingPricingModels(days = 30): Promise<string[]> {
  const port = getBackend().usage;
  if (port.missingPricingModels) return port.missingPricingModels(days);
  return [];
}

export async function collectUsage(
  onProgress?: (pct: number) => void,
): Promise<UsageCollectResult | void> {
  return getBackend().usage.collectUsage(onProgress);
}

export async function gatewayUsageQuery(
  q: GatewayUsageQuery = {},
): Promise<GatewayUsageRow[]> {
  return getBackend().usage.gatewayUsageQuery(q);
}

export async function gatewayUsageOverview(
  q: GatewayUsageQuery = {},
): Promise<GatewayUsageOverview> {
  return getBackend().usage.gatewayUsageOverview(q);
}
