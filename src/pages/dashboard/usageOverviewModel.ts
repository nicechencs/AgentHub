import { filterVisibleUsage, toHiddenIdSet } from '@/lib/agent-visibility';
import type {
  UsageOverview,
  UsageOverviewDistributionSlice,
  UsageOverviewMetrics,
} from '@/lib/backend/contracts/usage-types';
import type { AgentId, UsageRecord, UsageTrendPoint } from '@/lib/types';
import { usageTokenParts } from '@/lib/usage-tokens';

/** 日期筛选预设：today / 24h 均按 days=1 拉取，today 再按本地日历日收窄 */
export type DateRange = 'today' | '24h' | '7d' | '30d';

/** SQL window for overview / trend / table. `today` AND-s local midnight. */
export function usageWindowBound(
  dateRange: DateRange,
  now = new Date(),
): { days: number; since?: string } {
  if (dateRange === 'today') {
    const midnight = new Date(now.getFullYear(), now.getMonth(), now.getDate());
    return { days: 1, since: midnight.toISOString() };
  }
  const days = dateRange === '24h' ? 1 : dateRange === '7d' ? 7 : 30;
  return { days };
}

/** Drop hidden-agent slices (all-agents grouping) and re-sum metrics from the rest. */
export function filterHiddenUsageOverview(
  overview: UsageOverview,
  hiddenIds: Iterable<string>,
  groupedByAgent: boolean,
): UsageOverview {
  if (!groupedByAgent) return overview;
  const hidden = toHiddenIdSet(hiddenIds);
  if (hidden.size === 0) return overview;
  const distribution = overview.distribution.filter((slice) => !hidden.has(slice.key));
  return {
    metrics: sumOverviewMetrics(distribution),
    distribution,
    models: overview.models,
  };
}

export function sumOverviewMetrics(
  slices: readonly UsageOverviewDistributionSlice[],
): UsageOverviewMetrics {
  let billableInput = 0;
  let output = 0;
  let cache = 0;
  let costUsd = 0;
  for (const slice of slices) {
    billableInput += slice.billableInput;
    output += slice.output;
    cache += slice.cache;
    costUsd += slice.costUsd;
  }
  return { billableInput, output, cache, costUsd };
}

export function overviewToUsageMetrics(metrics: UsageOverviewMetrics): UsageMetrics {
  const fullInput = metrics.billableInput + metrics.cache;
  return {
    billableInput: metrics.billableInput,
    output: metrics.output,
    cacheRead: metrics.cache,
    fullInput,
    cost: metrics.costUsd,
    cacheHitPct: fullInput > 0 ? Math.round((metrics.cache / fullInput) * 100) : null,
  };
}

export function isLocalToday(iso: string, now = new Date()): boolean {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return false;
  return (
    d.getFullYear() === now.getFullYear() &&
    d.getMonth() === now.getMonth() &&
    d.getDate() === now.getDate()
  );
}

/** 时间窗 + 隐藏 Agent：模型下拉与后续筛选的公共窗口 */
export function filterWindowUsage(
  records: readonly UsageRecord[],
  dateRange: DateRange,
  hiddenIds: Iterable<string>,
  now = new Date(),
): UsageRecord[] {
  const scoped =
    dateRange !== 'today' ? [...records] : records.filter((r) => isLocalToday(r.timestamp, now));
  return filterVisibleUsage(scoped, hiddenIds);
}

export function filterByAgent(
  records: readonly UsageRecord[],
  agentFilter: AgentId | 'all',
): UsageRecord[] {
  if (agentFilter === 'all') return [...records];
  return records.filter((r) => r.agentId === agentFilter);
}

export function filterByModel(
  records: readonly UsageRecord[],
  modelFilter: string,
): UsageRecord[] {
  if (modelFilter === 'all' || modelFilter === '') return [...records];
  return records.filter((r) => r.model === modelFilter);
}

export function modelsFromRecords(records: readonly UsageRecord[]): string[] {
  return [...new Set(records.map((r) => r.model).filter((m) => m.length > 0))].sort((a, b) =>
    a.localeCompare(b),
  );
}

/** 当前模型不在窗口内时回退到全部，避免 Select 值悬空、图表空窗 */
export function coerceModelFilter(selected: string, available: readonly string[]): string {
  if (selected === 'all' || selected === '') return 'all';
  return available.includes(selected) ? selected : 'all';
}

export function sortUsageRowsDesc(records: readonly UsageRecord[]): UsageRecord[] {
  return [...records].sort((a, b) => b.timestamp.localeCompare(a.timestamp));
}

export interface UsageMetrics {
  billableInput: number;
  output: number;
  cacheRead: number;
  fullInput: number;
  cost: number;
  cacheHitPct: number | null;
}

export function computeUsageMetrics(rows: readonly UsageRecord[]): UsageMetrics {
  let billableInput = 0;
  let fullInput = 0;
  let output = 0;
  let cacheRead = 0;
  let cost = 0;
  for (const r of rows) {
    const p = usageTokenParts(r);
    billableInput += p.billableInput;
    fullInput += p.fullInput;
    output += r.outputTokens;
    cacheRead += p.cache;
    cost += r.costUsd;
  }
  return {
    billableInput,
    output,
    cacheRead,
    fullInput,
    cost,
    cacheHitPct: fullInput > 0 ? Math.round((cacheRead / fullInput) * 100) : null,
  };
}

export interface UsageDistributionSlice {
  key: string;
  label: string;
  color: string;
  tokens: number;
  cost: number;
}

const FALLBACK_COLOR = 'var(--text-muted)';

/** Attach catalog name/color to SQL distribution slices. */
export function decorateUsageDistribution(
  slices: readonly UsageOverviewDistributionSlice[],
  agentFilter: AgentId | 'all',
  catalog: Readonly<Record<string, { name: string; color: string }>>,
): UsageDistributionSlice[] {
  return slices.map((slice) => {
    const meta = catalog[agentFilter === 'all' ? slice.key : agentFilter];
    return {
      key: slice.key,
      label: agentFilter === 'all' ? (meta?.name ?? slice.key) : slice.key,
      color: meta?.color ?? FALLBACK_COLOR,
      tokens: slice.tokens,
      cost: slice.costUsd,
    };
  });
}

/** 全部 Agent 时按 agent 聚合；选中单个 Agent 时按模型聚合 */
export function buildUsageDistribution(
  rows: readonly UsageRecord[],
  agentFilter: AgentId | 'all',
  catalog: Readonly<Record<string, { name: string; color: string }>>,
): UsageDistributionSlice[] {
  const byKey = new Map<string, UsageDistributionSlice>();
  for (const r of rows) {
    const key = agentFilter === 'all' ? r.agentId : r.model;
    const meta = catalog[r.agentId];
    const entry = byKey.get(key) ?? {
      key,
      label: agentFilter === 'all' ? (meta?.name ?? r.agentId) : r.model,
      color: meta?.color ?? FALLBACK_COLOR,
      tokens: 0,
      cost: 0,
    };
    const p = usageTokenParts(r);
    entry.tokens += p.billableInput + p.cache + r.outputTokens;
    entry.cost += r.costUsd;
    byKey.set(key, entry);
  }
  return [...byKey.values()].sort((a, b) => b.tokens - a.tokens);
}

/**
 * 与后端 `usage_trend` 同一公式：按日、按 agent 累加 input+output。
 * 入参应已是时间/Agent/模型/可见性过滤后的 records，这样趋势与指标卡共用一份数据。
 */
export function buildUsageTrend(rows: readonly UsageRecord[]): UsageTrendPoint[] {
  const byDay = new Map<string, UsageTrendPoint>();
  for (const r of rows) {
    const day = r.timestamp.slice(0, 10);
    const point = byDay.get(day) ?? { date: day };
    const prev = typeof point[r.agentId] === 'number' ? (point[r.agentId] as number) : 0;
    point[r.agentId] = prev + r.inputTokens + r.outputTokens;
    byDay.set(day, point);
  }
  return [...byDay.values()].sort((a, b) => a.date.localeCompare(b.date));
}
