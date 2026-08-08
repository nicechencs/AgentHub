/**
 * Usage API façade — production via Tauri UsageService; mock under dev:mock.
 */
import { getBackend } from '@/app/runtime';
import type { UsageCollectResult } from '@/lib/backend/contracts/ports';
import type { AgentId, ParserHealth, UsageRecord, UsageTrendPoint } from '@/lib/types';

export type {
  UsageAvailability,
  UsageQuery,
} from '@/lib/backend/contracts/usage-types';
export type { UsageCollectResult };
import type { UsageAvailability, UsageQuery } from '@/lib/backend/contracts/usage-types';

export async function getUsageAvailability(): Promise<UsageAvailability> {
  return getBackend().usage.getAvailability();
}

export async function queryUsage(q: UsageQuery): Promise<UsageRecord[]> {
  return getBackend().usage.queryUsage(q);
}

export async function usageTrend(
  days: number,
  agentId?: AgentId | 'all',
): Promise<UsageTrendPoint[]> {
  return getBackend().usage.usageTrend(days, agentId);
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
