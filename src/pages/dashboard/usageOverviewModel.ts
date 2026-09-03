import { filterVisibleUsage, toHiddenIdSet } from '@/lib/agent-visibility';
import type {
  UsageOverview,
  UsageOverviewDistributionSlice,
  UsageOverviewMetrics,
} from '@/lib/backend/contracts/usage-types';
import type { AgentKey, UsageRecord } from '@/lib/types';
import type { UiLanguage } from '@/lib/i18n';
import { canonicalUsageModel, usageModelsMatch } from '@/lib/usage-model';
import { usageTokenParts } from '@/lib/usage-tokens';
import type { UsageTrendGroup } from './usageTrendChartModel';

/** 日期筛选预设：today / 24h 均按 days=1 拉取，today 再按本地日历日收窄 */
export type DateRange = 'today' | '24h' | '7d' | '30d';

/** 总览用量筛选：进程内记忆，关应用后回到默认 */
export interface UsageOverviewFilters {
  dateRange: DateRange;
  agentFilter: AgentKey | 'all';
  modelFilter: string;
  trendGroup: UsageTrendGroup;
}

export const DEFAULT_USAGE_OVERVIEW_FILTERS: UsageOverviewFilters = {
  dateRange: '7d',
  agentFilter: 'all',
  modelFilter: 'all',
  trendGroup: 'agent',
};

let rememberedFilters: UsageOverviewFilters = { ...DEFAULT_USAGE_OVERVIEW_FILTERS };

export function rememberedUsageFilters(): UsageOverviewFilters {
  return { ...rememberedFilters };
}

export function rememberUsageFilters(next: UsageOverviewFilters): void {
  rememberedFilters = { ...next };
}

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

/** Local start/end of the selected window (matches overview/trend bounds). */
export function usageWindowSpan(
  dateRange: DateRange,
  now = new Date(),
): { start: Date; end: Date } {
  if (dateRange === 'today') {
    return { start: new Date(now.getFullYear(), now.getMonth(), now.getDate()), end: now };
  }
  if (dateRange === '24h') {
    return { start: new Date(now.getTime() - 24 * 3600 * 1000), end: now };
  }
  const days = dateRange === '7d' ? 7 : 30;
  const rollingStart = new Date(now.getTime() - days * 24 * 3600 * 1000);
  return {
    start: new Date(rollingStart.getFullYear(), rollingStart.getMonth(), rollingStart.getDate()),
    end: now,
  };
}

const EN_MONTHS = [
  'Jan',
  'Feb',
  'Mar',
  'Apr',
  'May',
  'Jun',
  'Jul',
  'Aug',
  'Sep',
  'Oct',
  'Nov',
  'Dec',
] as const;

function formatUsageDate(date: Date, lang: UiLanguage): string {
  if (lang === 'zh') return `${date.getMonth() + 1}月${date.getDate()}日`;
  return `${EN_MONTHS[date.getMonth()]} ${date.getDate()}`;
}

function isSameLocalDay(a: Date, b: Date): boolean {
  return (
    a.getFullYear() === b.getFullYear() &&
    a.getMonth() === b.getMonth() &&
    a.getDate() === b.getDate()
  );
}

/** Visible range next to the time presets, e.g. `8月28日 – 9月3日`. */
export function formatUsageWindowLabel(
  dateRange: DateRange,
  lang: UiLanguage,
  now = new Date(),
): string {
  const { start, end } = usageWindowSpan(dateRange, now);
  const left = formatUsageDate(start, lang);
  if (isSameLocalDay(start, end)) return left;
  return `${left} – ${formatUsageDate(end, lang)}`;
}

/** Drop omitted-agent slices (hidden / uninstalled) when grouping by agent, then re-sum. */
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
  let cacheRead = 0;
  let cacheWrite = 0;
  let costUsd = 0;
  for (const slice of slices) {
    billableInput += slice.billableInput;
    output += slice.output;
    cacheRead += slice.cacheRead;
    cacheWrite += slice.cacheWrite;
    costUsd += slice.costUsd;
  }
  return { billableInput, output, cacheRead, cacheWrite, costUsd };
}

export function overviewToUsageMetrics(metrics: UsageOverviewMetrics): UsageMetrics {
  const cache = metrics.cacheRead + metrics.cacheWrite;
  const fullInput = metrics.billableInput + cache;
  return {
    billableInput: metrics.billableInput,
    output: metrics.output,
    cacheRead: metrics.cacheRead,
    cacheWrite: metrics.cacheWrite,
    fullInput,
    cost: metrics.costUsd,
    // Combined write+read as a share of the prompt, not a hit rate.
    cacheHitPct: fullInput > 0 ? Math.round((cache / fullInput) * 100) : null,
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

/** 时间窗 + 省略 Agent（隐藏 / 未安装）：模型下拉与后续筛选的公共窗口 */
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
  agentFilter: AgentKey | 'all',
): UsageRecord[] {
  if (agentFilter === 'all') return [...records];
  return records.filter((r) => r.agentId === agentFilter);
}

export function filterByModel(
  records: readonly UsageRecord[],
  modelFilter: string,
): UsageRecord[] {
  if (modelFilter === 'all' || modelFilter === '') return [...records];
  return records.filter((r) => usageModelsMatch(r.model, modelFilter));
}

/** 当前模型不在窗口内时回退到全部，避免 Select 值悬空、图表空窗 */
export function coerceModelFilter(selected: string, available: readonly string[]): string {
  if (selected === 'all' || selected === '') return 'all';
  return available.includes(selected) ? selected : 'all';
}

/**
 * overview 未到之前保留记忆中的模型，避免空列表把筛选打回「全部」。
 */
export function resolveUsageModelFilter(
  selected: string,
  available: readonly string[],
  modelsReady: boolean,
): string {
  if (!modelsReady) return selected === '' ? 'all' : selected;
  return coerceModelFilter(selected, available);
}

/** 记忆中的模型还不在窗口列表时先挂上，Select 触发器才有文案。 */
export function usageModelSelectOptions(
  selected: string,
  available: readonly string[],
): string[] {
  if (selected === 'all' || selected === '' || available.includes(selected)) {
    return [...available];
  }
  return [selected, ...available];
}

export function sortUsageRowsDesc(records: readonly UsageRecord[]): UsageRecord[] {
  return [...records].sort((a, b) => b.timestamp.localeCompare(a.timestamp));
}

export interface UsageMetrics {
  billableInput: number;
  output: number;
  cacheRead: number;
  cacheWrite: number;
  fullInput: number;
  cost: number;
  cacheHitPct: number | null;
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
  agentFilter: AgentKey | 'all',
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
  agentFilter: AgentKey | 'all',
  catalog: Readonly<Record<string, { name: string; color: string }>>,
): UsageDistributionSlice[] {
  const byKey = new Map<string, UsageDistributionSlice>();
  for (const r of rows) {
    const key =
      agentFilter === 'all' ? r.agentId : canonicalUsageModel(r.model) || r.model;
    const meta = catalog[r.agentId];
    const entry = byKey.get(key) ?? {
      key,
      label: agentFilter === 'all' ? (meta?.name ?? r.agentId) : key,
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

