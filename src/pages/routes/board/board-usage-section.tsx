import { useEffect, useMemo, useState } from 'react';
import { Link } from 'react-router-dom';
import {
  Area,
  AreaChart,
  CartesianGrid,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts';
import { PageSection } from '@/components/layout/PageSection';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { AgentDot } from '@/components/shared/AgentDot';
import { ErrorState } from '@/components/shared/ErrorState';
import { useI18n } from '@/components/shared/LanguageProvider';
import { useTheme } from '@/components/shared/ThemeProvider';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Skeleton } from '@/components/ui/skeleton';
import { tooltipSurfaceStyle } from '@/components/ui/tooltip';
import { resolveAgentMeta } from '@/config/agents';
import type { AdapterProfile } from '@/lib/backend/contracts/adapter';
import type { MessageKey } from '@/lib/i18n';
import { resolveTheme } from '@/lib/theme';
import { USAGE_COLLECTED_EVENT } from '@/lib/usage-sync';
import { formatTrendTick, formatTrendTooltipLabel } from '@/lib/usage-trend';
import { cn, fmtTokens } from '@/lib/utils';
import {
  resolveUsageModelFilter,
  usageModelSelectOptions,
} from '@/pages/dashboard/usageOverviewModel';
import { activityHref } from '@/pages/routes/board/board-view-model';
import { resolveChartColor, typeScalePx } from '@/styles/tokens';
import {
  BOARD_SURFACES,
  boardUsageWindow,
  buildBoardUsageEntries,
  buildGatewayDistribution,
  buildGatewayTrend,
  filterGatewayUsageRows,
  profileToEntryIdMap,
  deriveBoardGroupBy,
  rememberBoardUsageFilters,
  rememberedBoardUsageFilters,
  seriesKeyForRow,
  summarizeGatewayUsage,
  type BoardUsageRange,
  type BoardUsageSurface,
} from './board-usage-model';
import { useBoardUsageStats } from './use-board-usage';

const DATE_RANGE_OPTIONS: { value: BoardUsageRange }[] = [
  { value: 'today' },
  { value: '24h' },
  { value: '7d' },
  { value: '30d' },
];

const DATE_RANGE_LABEL_KEYS: Record<BoardUsageRange, MessageKey> = {
  today: 'dashboard.range.today',
  '24h': 'dashboard.range.last24h',
  '7d': 'dashboard.range.last7d',
  '30d': 'dashboard.range.last30d',
};

const SURFACE_LABEL_KEYS: Record<BoardUsageSurface, MessageKey> = {
  messages: 'routes.pool.surface.messages',
  responses: 'routes.pool.surface.responses',
  chat: 'routes.pool.surface.chatCompletions',
};

const SURFACE_COLORS: Record<BoardUsageSurface, string> = {
  messages: 'var(--agent-claude)',
  responses: 'var(--agent-codex)',
  chat: 'var(--agent-grok)',
};

const FALLBACK_COLOR = 'var(--text-muted)';

export function BoardUsageSection({
  profiles,
  hiddenTargetIds,
  refreshKey = 0,
}: {
  profiles: readonly AdapterProfile[];
  hiddenTargetIds: ReadonlySet<string>;
  refreshKey?: number;
}) {
  const { t } = useI18n();
  const { theme } = useTheme();
  const chartScheme = resolveTheme(theme);
  const [dateRange, setDateRange] = useState<BoardUsageRange>(
    () => rememberedBoardUsageFilters().dateRange,
  );
  const [entryId, setEntryId] = useState(() => rememberedBoardUsageFilters().entryId);
  const [surface, setSurface] = useState(() => rememberedBoardUsageFilters().surface);
  const [modelFilter, setModelFilter] = useState(
    () => rememberedBoardUsageFilters().modelFilter,
  );
  const [retryKey, setRetryKey] = useState(0);
  const [collectKey, setCollectKey] = useState(0);

  useEffect(() => {
    const onCollected = () => setCollectKey((key) => key + 1);
    window.addEventListener(USAGE_COLLECTED_EVENT, onCollected);
    return () => window.removeEventListener(USAGE_COLLECTED_EVENT, onCollected);
  }, []);

  const entries = useMemo(
    () => buildBoardUsageEntries(profiles, hiddenTargetIds),
    [profiles, hiddenTargetIds],
  );
  const entryMap = useMemo(() => profileToEntryIdMap(entries), [entries]);

  useEffect(() => {
    if (entryId === 'all') return;
    if (!entries.some((entry) => entry.id === entryId)) setEntryId('all');
  }, [entryId, entries]);

  const { days, since } = useMemo(() => boardUsageWindow(dateRange), [dateRange]);
  const usage = useBoardUsageStats({
    enabled: entries.length > 0,
    since,
    refreshKey: refreshKey + retryKey + collectKey,
  });

  const selectedEntry = entries.find((entry) => entry.id === entryId) ?? null;
  const scopedRows = useMemo(() => {
    if (usage.status !== 'ready') return [];
    return filterGatewayUsageRows(usage.rows, {
      profileIds: selectedEntry?.profileIds,
      surface,
      model: modelFilter,
    });
  }, [usage, selectedEntry, surface, modelFilter]);

  const windowRows = useMemo(() => {
    if (usage.status !== 'ready') return [];
    return filterGatewayUsageRows(usage.rows, {
      profileIds: selectedEntry?.profileIds,
      surface,
    });
  }, [usage, selectedEntry, surface]);

  const totals = useMemo(() => summarizeGatewayUsage(scopedRows), [scopedRows]);
  const modelOptions = useMemo(
    () => summarizeGatewayUsage(windowRows).modelNames,
    [windowRows],
  );
  const modelsReady = usage.status === 'ready';
  const effectiveModelFilter = resolveUsageModelFilter(
    modelFilter,
    modelOptions,
    modelsReady,
  );
  const modelSelectOptions = usageModelSelectOptions(effectiveModelFilter, modelOptions);
  const effectiveGroupBy = deriveBoardGroupBy(entryId);

  useEffect(() => {
    if (!modelsReady) return;
    if (modelFilter !== effectiveModelFilter) setModelFilter(effectiveModelFilter);
  }, [modelsReady, modelFilter, effectiveModelFilter]);

  useEffect(() => {
    rememberBoardUsageFilters({
      dateRange,
      entryId,
      surface,
      modelFilter,
    });
  }, [dateRange, entryId, surface, modelFilter]);

  const rangeLabel = t(DATE_RANGE_LABEL_KEYS[dateRange]);
  const seriesMeta = useMemo(() => {
    const labels: Record<string, { label: string; color: string }> = {};
    for (const item of entries) {
      labels[item.id] = {
        label: item.name,
        color: resolveAgentMeta(item.targetAgentId).color,
      };
    }
    for (const item of BOARD_SURFACES) {
      labels[item] = { label: t(SURFACE_LABEL_KEYS[item]), color: SURFACE_COLORS[item] };
    }
    for (const model of totals.modelNames) {
      labels[model] = {
        label: model,
        color: selectedEntry
          ? resolveAgentMeta(selectedEntry.targetAgentId).color
          : FALLBACK_COLOR,
      };
    }
    return labels;
  }, [entries, totals.modelNames, selectedEntry, t]);

  const trendSeries = useMemo(() => {
    const list = selectedEntry ? [selectedEntry] : entries;
    return list.map((item) => ({
      key: item.id,
      label: item.name,
      color: seriesMeta[item.id]?.color ?? FALLBACK_COLOR,
    }));
  }, [selectedEntry, entries, seriesMeta]);

  const rangedTrend = useMemo(() => {
    if (usage.status !== 'ready') return [];
    return buildGatewayTrend(
      scopedRows,
      days,
      since,
      trendSeries.map((item) => item.key),
      (item) => seriesKeyForRow(item, 'entry', entryMap),
    );
  }, [usage, scopedRows, days, since, trendSeries, entryMap]);

  const distribution = useMemo(
    () => buildGatewayDistribution(scopedRows, effectiveGroupBy, entryMap, seriesMeta),
    [scopedRows, effectiveGroupBy, entryMap, seriesMeta],
  );
  const maxTokens = distribution[0]?.tokens ?? 0;
  const distTitle =
    effectiveGroupBy === 'entry'
      ? t('routes.board.distByEntry')
      : t('routes.board.distByModel');

  return (
    <PageSection title={t('routes.board.usageSection')}>
      <div className={pageRhythm.chromeRow}>
        <Select value={entryId} onValueChange={setEntryId}>
          <SelectTrigger className="w-40" aria-label={t('routes.board.entryFilterAria')}>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="all">{t('routes.board.allEntries')}</SelectItem>
            {entries.map((entry) => (
              <SelectItem key={entry.id} value={entry.id}>
                {entry.name}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <Select value={surface} onValueChange={setSurface}>
          <SelectTrigger className="w-36" aria-label={t('routes.board.surfaceFilterAria')}>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="all">{t('routes.board.allSurfaces')}</SelectItem>
            {BOARD_SURFACES.map((item) => (
              <SelectItem key={item} value={item}>
                {t(SURFACE_LABEL_KEYS[item])}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <Select value={effectiveModelFilter} onValueChange={setModelFilter}>
          <SelectTrigger className="w-44" aria-label={t('routes.board.modelFilterAria')}>
            <SelectValue placeholder={t('dashboard.page.allModels')} />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="all">{t('dashboard.page.allModels')}</SelectItem>
            {modelSelectOptions.map((model) => (
              <SelectItem key={model} value={model}>
                {model}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <Select
          value={dateRange}
          onValueChange={(value) => setDateRange(value as BoardUsageRange)}
        >
          <SelectTrigger className="w-32" aria-label={t('routes.board.rangeAria')}>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {DATE_RANGE_OPTIONS.map((option) => (
              <SelectItem key={option.value} value={option.value}>
                {t(DATE_RANGE_LABEL_KEYS[option.value])}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <div className={pageRhythm.chromeActions}>
          <Link to={activityHref({})} className="text-meta text-secondary hover:text-primary">
            {t('routes.board.openLog')}
          </Link>
        </div>
      </div>

      {usage.status === 'loading' || (usage.status === 'idle' && entries.length > 0) ? (
        <UsageOverviewSkeleton />
      ) : usage.status === 'idle' ? (
        <p className="text-meta text-muted">{t('routes.board.usageEmpty')}</p>
      ) : usage.status === 'unavailable' ? (
        <Card className="p-6">
          <p className="text-sm font-medium text-primary">{t('routes.board.usageUnavailable')}</p>
          {usage.reason ? (
            <p className="mt-1 text-xs text-secondary">
              {t('routes.board.usageUnavailableReason', { reason: usage.reason })}
            </p>
          ) : null}
        </Card>
      ) : usage.status === 'error' ? (
        <ErrorState
          compact
          error={t('routes.board.usageUnavailable')}
          onRetry={() => setRetryKey((key) => key + 1)}
        />
      ) : (
        <div
          className={cn(
            pageRhythm.blocks,
            usage.refreshing ? 'opacity-60 transition-opacity' : 'transition-opacity',
          )}
        >
          <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-5">
            <MetricCard
              label={t('routes.board.statRequests')}
              value={totals.requestCount.toLocaleString()}
            />
            <MetricCard
              label={t('dashboard.page.metricInput', { range: rangeLabel })}
              value={fmtTokens(totals.inputTokens)}
            />
            <MetricCard
              label={t('dashboard.page.metricOutput', { range: rangeLabel })}
              value={fmtTokens(totals.outputTokens)}
            />
            <MetricCard
              label={t('routes.board.statCache', { range: rangeLabel })}
              value={fmtTokens(totals.cachedInputTokens)}
            />
            <MetricCard
              label={t('routes.board.statFailed')}
              value={totals.failedCount.toLocaleString()}
            />
          </div>

          <Card>
            <CardHeader>
              <CardTitle>
                {t('dashboard.page.tokenUsageTitle', { range: rangeLabel })}
              </CardTitle>
              <p className="text-xs text-muted">
                {t('routes.board.tokenUsageSummary', {
                  requests: totals.requestCount.toLocaleString(),
                  in: fmtTokens(totals.inputTokens),
                  out: fmtTokens(totals.outputTokens),
                })}
              </p>
            </CardHeader>
            <CardContent>
              <div className="h-56">
                <ResponsiveContainer width="100%" height="100%">
                  <AreaChart data={rangedTrend} margin={{ top: 4, right: 8, bottom: 0, left: 0 }}>
                    <defs>
                      {trendSeries.map((series) => {
                        const color = resolveChartColor(series.color, chartScheme);
                        return (
                          <linearGradient
                            key={`grad-${series.key}`}
                            id={`board-usage-fill-${series.key}`}
                            x1="0"
                            y1="0"
                            x2="0"
                            y2="1"
                          >
                            <stop offset="0%" stopColor={color} stopOpacity={0.18} />
                            <stop offset="100%" stopColor={color} stopOpacity={0.02} />
                          </linearGradient>
                        );
                      })}
                    </defs>
                    <CartesianGrid
                      stroke="var(--border)"
                      strokeDasharray="3 3"
                      vertical={false}
                      strokeOpacity={0.6}
                    />
                    <XAxis
                      dataKey="date"
                      tick={{ fill: 'var(--text-muted)', fontSize: typeScalePx('meta') }}
                      tickLine={false}
                      axisLine={{ stroke: 'var(--border)' }}
                      minTickGap={28}
                      interval="preserveStartEnd"
                      tickFormatter={(d: string) => formatTrendTick(d)}
                    />
                    <YAxis
                      tick={{ fill: 'var(--text-muted)', fontSize: typeScalePx('meta') }}
                      tickLine={false}
                      axisLine={false}
                      tickFormatter={(v: number) => fmtTokens(v)}
                      width={48}
                    />
                    <Tooltip
                      contentStyle={tooltipSurfaceStyle()}
                      labelStyle={{
                        color: 'var(--text-secondary)',
                        fontSize: 'var(--font-meta-size)',
                      }}
                      itemStyle={{
                        fontSize: 'var(--font-meta-size)',
                      }}
                      labelFormatter={(label) => formatTrendTooltipLabel(String(label))}
                      formatter={(value, name) => {
                        const tokens = Number(value);
                        if (!tokens) return null;
                        const series = trendSeries.find((item) => item.key === name);
                        return [fmtTokens(tokens), series?.label ?? String(name)];
                      }}
                    />
                    {trendSeries.map((series) => (
                      <Area
                        key={series.key}
                        type="monotone"
                        dataKey={series.key}
                        stroke={resolveChartColor(series.color, chartScheme)}
                        strokeWidth={1.5}
                        fill={`url(#board-usage-fill-${series.key})`}
                        isAnimationActive={false}
                        activeDot={{ r: 3, strokeWidth: 0 }}
                      />
                    ))}
                  </AreaChart>
                </ResponsiveContainer>
              </div>
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle>{distTitle}</CardTitle>
            </CardHeader>
            <CardContent>
              {distribution.length === 0 ? (
                <p className="py-4 text-sm text-secondary">{t('dashboard.page.noData')}</p>
              ) : (
                <ul className="space-y-1.5">
                  {distribution.map((row) => (
                    <li key={row.key} className="flex h-7 items-center gap-3">
                      <span className="flex min-w-0 flex-1 items-center gap-1.5 truncate text-sm">
                        <AgentDot color={row.color} size="md" title={null} />
                        <span className="truncate">{row.label}</span>
                      </span>
                      {row.tokens === 0 ? (
                        <span className="shrink-0 text-xs text-muted">
                          {t('dashboard.page.noDataShort')}
                        </span>
                      ) : (
                        <>
                          <div className="h-1.5 w-32 shrink-0 overflow-hidden rounded-full bg-subtle sm:w-40">
                            <div
                              className="h-full rounded-full"
                              style={{
                                width: maxTokens > 0 ? `${(row.tokens / maxTokens) * 100}%` : 0,
                                backgroundColor: row.color,
                              }}
                            />
                          </div>
                          <span className="w-20 shrink-0 text-right font-mono text-xs text-secondary">
                            {fmtTokens(row.tokens)}
                          </span>
                          <span className="w-16 shrink-0 text-right font-mono text-xs text-muted">
                            {t('routes.board.distRequests', { count: row.requests })}
                          </span>
                        </>
                      )}
                    </li>
                  ))}
                </ul>
              )}
            </CardContent>
          </Card>
        </div>
      )}
    </PageSection>
  );
}

function MetricCard({ label, value }: { label: string; value: string }) {
  return (
    <Card className="p-3">
      <p className="text-xs text-muted">{label}</p>
      <p className="mt-1 text-title font-semibold tracking-tight">{value}</p>
    </Card>
  );
}

function UsageOverviewSkeleton() {
  return (
    <div className={pageRhythm.blocks}>
      <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-5">
        {Array.from({ length: 5 }).map((_, i) => (
          <Skeleton key={i} className="h-20" />
        ))}
      </div>
      <Card className="p-4">
        <Skeleton className="h-4 w-32" />
        <Skeleton className="mt-4 h-56 w-full" />
      </Card>
      <Card className="space-y-3 p-4">
        <Skeleton className="h-4 w-28" />
        <Skeleton className="h-4 w-full" />
        <Skeleton className="h-4 w-full" />
      </Card>
    </div>
  );
}
