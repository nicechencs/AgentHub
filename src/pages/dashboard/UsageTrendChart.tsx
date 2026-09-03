import { useCallback, useMemo, useRef } from 'react';
import {
  Area,
  AreaChart,
  CartesianGrid,
  ReferenceLine,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts';

import { useI18n } from '@/components/shared/LanguageProvider';
import { SegmentedControl } from '@/components/shared/SegmentedControl';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import type { AgentKey, UsageTrendPoint } from '@/lib/types';
import {
  formatTrendTick,
  sortUsageTrendTooltipItems,
  todayTrendBucket,
  trendPointGrain,
  zeroFillTrendSeries,
} from '@/lib/usage-trend';
import { fmtTokens } from '@/lib/utils';
import { resolveChartColor, typeScalePx, type ThemeScheme } from '@/styles/tokens';

import {
  USAGE_TREND_Y_AXIS_WIDTH,
  UsageTrendTooltipCard,
  useUsageTrendHover,
  type ChartHoverState,
} from './UsageTrendTooltip';
import {
  accumulateTrendSeries,
  costFromTrendPoint,
  foldTrendTail,
  fmtTrendCost,
  isModelOtherKey,
  listTrendSeriesKeys,
  modelSeriesColor,
  rankTrendSeriesKeys,
  sumTrendSeriesTokens,
  trendSharePct,
  type UsageTrendGroup,
} from './usageTrendChartModel';

export interface UsageTrendSeriesMeta {
  id: string;
  name: string;
  color: string;
}

export function UsageTrendChart({
  dayLabel,
  summary,
  group,
  onGroupChange,
  agentPoints,
  agentSeries,
  modelPoints,
  chartScheme,
}: {
  dayLabel: string;
  summary: string;
  group: UsageTrendGroup;
  onGroupChange: (group: UsageTrendGroup) => void;
  agentPoints: readonly UsageTrendPoint[];
  agentSeries: readonly UsageTrendSeriesMeta[];
  modelPoints: readonly UsageTrendPoint[];
  chartScheme: ThemeScheme;
}) {
  const { t } = useI18n();
  const rankedModelKeys = useMemo(() => {
    const keys = listTrendSeriesKeys(modelPoints);
    return rankTrendSeriesKeys(modelPoints, keys);
  }, [modelPoints]);
  const foldedModels = useMemo(
    () => foldTrendTail(modelPoints, rankedModelKeys),
    [modelPoints, rankedModelKeys],
  );
  const modelDailyPoints = useMemo(
    () => zeroFillTrendSeries(foldedModels.points, foldedModels.keys),
    [foldedModels],
  );
  const modelChartPoints = useMemo(
    () => accumulateTrendSeries(modelDailyPoints, foldedModels.keys),
    [modelDailyPoints, foldedModels.keys],
  );
  const modelSeries = useMemo(
    () =>
      foldedModels.keys.map((key, index) => ({
        id: key,
        name: isModelOtherKey(key) ? t('dashboard.page.trendOther') : key,
        color: modelSeriesColor(index),
      })),
    [foldedModels.keys, t],
  );
  const byModel = group === 'model';
  const series = byModel ? modelSeries : agentSeries;
  const data = byModel ? modelChartPoints : [...agentPoints];
  const title = t('dashboard.page.tokenUsageTitle', { range: dayLabel });
  const chartSummary = byModel ? t('dashboard.page.tokenUsageCumulativeSummary') : summary;

  const resolveName = useCallback(
    (key: string) => {
      if (byModel) {
        if (isModelOtherKey(key)) return t('dashboard.page.trendOther');
        return key;
      }
      const hit = agentSeries.find((item) => item.id === key);
      return hit?.name ?? key;
    },
    [agentSeries, byModel, t],
  );

  const dailyByDate = useMemo(() => {
    const src = byModel ? modelDailyPoints : agentPoints;
    return new Map(src.map((point) => [point.date, point]));
  }, [agentPoints, byModel, modelDailyPoints]);

  const buildTip = useCallback(
    (state: ChartHoverState) => {
      const label = String(state?.activeLabel ?? '');
      const daily = dailyByDate.get(label);
      if (!daily) return null;
      const keys = series.map((item) => item.id);
      const dailyTotal = sumTrendSeriesTokens(daily, keys);
      const items = sortUsageTrendTooltipItems(
        series.map((item) => {
          const tokens = Number(daily[item.id]) || 0;
          const cost = byModel ? costFromTrendPoint(daily, item.id) : 0;
          return {
            key: item.id,
            name: resolveName(item.id),
            tokens,
            color: byModel ? item.color : resolveChartColor(item.color, chartScheme),
            extra: cost > 0 ? fmtTrendCost(cost) : undefined,
            share: trendSharePct(tokens, dailyTotal),
          };
        }),
      );
      if (!items.length) return null;
      return {
        label,
        items,
        dailyTotal,
        cumulativeTotal: byModel
          ? sumTrendSeriesTokens(
              data.find((point) => point.date === label),
              keys,
            )
          : undefined,
      };
    },
    [byModel, chartScheme, dailyByDate, data, resolveName, series],
  );

  const trendHover = useUsageTrendHover(resolveName, { buildTip });
  const plotRef = useRef<HTMLDivElement>(null);
  const onPlotMouseMove = useCallback(
    (state: ChartHoverState) => {
      trendHover.onChartMouseMove(state, plotRef.current);
    },
    [trendHover.onChartMouseMove],
  );

  const todayKey = useMemo(() => {
    const sample = data[0]?.date;
    if (!sample) return null;
    return todayTrendBucket(new Date(), trendPointGrain(sample));
  }, [data]);
  const showToday = Boolean(todayKey && data.some((point) => point.date === todayKey));
  const todayIsLast = Boolean(todayKey && data.at(-1)?.date === todayKey);

  const axis = (
    <>
      <CartesianGrid stroke="var(--border)" strokeDasharray="3 3" vertical={false} strokeOpacity={0.6} />
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
        width={USAGE_TREND_Y_AXIS_WIDTH}
      />
      <Tooltip cursor={{ stroke: 'var(--border)', strokeWidth: 1 }} content={() => null} />
      {showToday && todayKey ? (
        <ReferenceLine x={todayKey} stroke="var(--text-muted)" strokeDasharray="4 4" />
      ) : null}
    </>
  );

  return (
    <Card>
      <CardHeader className="px-5 py-4">
        <div className="min-w-0">
          <CardTitle>{title}</CardTitle>
          <p className="text-meta text-muted">{chartSummary}</p>
        </div>
        <div className="flex shrink-0 flex-col items-end gap-1.5">
          <SegmentedControl
            size="sm"
            aria-label={t('dashboard.page.trendGroupAria')}
            value={group}
            onChange={onGroupChange}
            options={[
              { value: 'agent', label: t('dashboard.page.trendGroupAgent') },
              { value: 'model', label: t('dashboard.page.trendGroupModel') },
            ]}
          />
        </div>
      </CardHeader>
      <CardContent className="px-5 pb-5">
        <div ref={plotRef} className="relative h-72">
          <ResponsiveContainer width="100%" height="100%">
            {byModel ? (
              <AreaChart
                data={data}
                margin={{ top: 16, right: 12, bottom: 0, left: 0 }}
                onMouseMove={onPlotMouseMove}
                onMouseLeave={trendHover.onChartMouseLeave}
              >
                {axis}
                {series.map((item) => (
                  <Area
                    key={item.id}
                    type="monotone"
                    dataKey={item.id}
                    name={item.name}
                    stackId="model-usage"
                    stroke={item.color}
                    strokeWidth={1.5}
                    fill={item.color}
                    fillOpacity={0.45}
                    isAnimationActive={false}
                    activeDot={{ r: 3, strokeWidth: 0 }}
                  />
                ))}
              </AreaChart>
            ) : (
              <AreaChart
                data={data}
                margin={{ top: 16, right: 12, bottom: 0, left: 0 }}
                onMouseMove={onPlotMouseMove}
                onMouseLeave={trendHover.onChartMouseLeave}
              >
                <defs>
                  {series.map((item) => {
                    const color = resolveChartColor(item.color, chartScheme);
                    return (
                      <linearGradient
                        key={`grad-${item.id}`}
                        id={`usage-fill-${item.id}`}
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
                {axis}
                {series.map((item) => (
                  <Area
                    key={item.id}
                    type="monotone"
                    dataKey={item.id}
                    name={item.name}
                    stroke={resolveChartColor(item.color, chartScheme)}
                    strokeWidth={1.5}
                    fill={`url(#usage-fill-${item.id})`}
                    isAnimationActive={false}
                    activeDot={{ r: 3, strokeWidth: 0 }}
                  />
                ))}
              </AreaChart>
            )}
          </ResponsiveContainer>
          {todayIsLast ? (
            <div className="pointer-events-none absolute right-2 top-1 rounded-full bg-subtle px-2 py-0.5 text-meta text-muted">
              {t('dashboard.page.trendToday')}
            </div>
          ) : null}
          {trendHover.tip ? (
            <UsageTrendTooltipCard
              label={trendHover.tip.label}
              items={trendHover.tip.items}
              dailyTotal={trendHover.tip.dailyTotal}
              cumulativeTotal={trendHover.tip.cumulativeTotal}
              dailyTotalLabel={t('dashboard.page.trendDailyTotal')}
              cumulativeTotalLabel={t('dashboard.page.trendCumulativeTotal')}
              x={trendHover.tip.x}
              y={trendHover.tip.y}
              containerWidth={trendHover.tip.containerWidth}
              containerHeight={trendHover.tip.containerHeight}
              onMouseEnter={trendHover.onTipMouseEnter}
              onMouseLeave={trendHover.onTipMouseLeave}
            />
          ) : null}
        </div>
        {series.length > 0 ? (
          <div className="mt-3 flex flex-wrap gap-x-4 gap-y-1">
            {series.map((item) => (
              <span
                key={item.id}
                className="inline-flex items-center gap-1.5 text-meta text-secondary"
              >
                <span
                  className="h-2 w-2 shrink-0 rounded-full"
                  style={{
                    backgroundColor: byModel
                      ? item.color
                      : resolveChartColor(item.color, chartScheme),
                  }}
                />
                <span className="max-w-[12rem] truncate">{item.name}</span>
              </span>
            ))}
          </div>
        ) : null}
      </CardContent>
    </Card>
  );
}

export function agentTrendSeries(
  agentIds: readonly AgentKey[],
  catalog: Readonly<Record<string, { name: string; color: string }>>,
): UsageTrendSeriesMeta[] {
  return agentIds.flatMap((id) => {
    const meta = catalog[id];
    return meta ? [{ id, name: meta.name, color: meta.color }] : [];
  });
}
