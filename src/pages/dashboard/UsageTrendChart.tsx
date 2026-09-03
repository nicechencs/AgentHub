import { useCallback, useMemo } from 'react';
import {
  Area,
  AreaChart,
  CartesianGrid,
  Line,
  LineChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts';

import { useI18n } from '@/components/shared/LanguageProvider';
import { SegmentedControl } from '@/components/shared/SegmentedControl';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import type { AgentKey, UsageTrendPoint } from '@/lib/types';
import { formatTrendTick, zeroFillTrendSeries } from '@/lib/usage-trend';
import { fmtTokens } from '@/lib/utils';
import { resolveChartColor, typeScalePx, type ThemeScheme } from '@/styles/tokens';

import {
  USAGE_TREND_Y_AXIS_WIDTH,
  UsageTrendTooltipCard,
  useUsageTrendHover,
} from './UsageTrendTooltip';
import {
  costFromTrendPoint,
  fmtTrendCost,
  listTrendSeriesKeys,
  modelSeriesColor,
  rankTrendSeriesKeys,
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
  const modelKeys = useMemo(() => {
    const keys = listTrendSeriesKeys(modelPoints);
    return rankTrendSeriesKeys(modelPoints, keys);
  }, [modelPoints]);
  const modelSeries = useMemo(
    () =>
      modelKeys.map((key, index) => ({
        id: key,
        name: key,
        color: modelSeriesColor(index),
      })),
    [modelKeys],
  );
  const modelChartPoints = useMemo(
    () => zeroFillTrendSeries(modelPoints, modelKeys),
    [modelPoints, modelKeys],
  );
  const byModel = group === 'model';
  const series = byModel ? modelSeries : agentSeries;
  const data = byModel ? modelChartPoints : [...agentPoints];
  const title = t('dashboard.page.tokenUsageTitle', { range: dayLabel });

  const resolveName = useCallback(
    (key: string) => {
      if (byModel) return key;
      const hit = agentSeries.find((item) => item.id === key);
      return hit?.name ?? key;
    },
    [agentSeries, byModel],
  );
  const extraFor = useCallback(
    (key: string, _value: number, payload?: Record<string, unknown>) => {
      if (!byModel) return undefined;
      const cost = costFromTrendPoint(
        payload as UsageTrendPoint | undefined,
        key,
      );
      return cost > 0 ? fmtTrendCost(cost) : undefined;
    },
    [byModel],
  );
  const trendHover = useUsageTrendHover(resolveName, {
    extraFor: byModel ? extraFor : undefined,
  });

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
    </>
  );

  return (
    <Card>
      <CardHeader>
        <div className="min-w-0">
          <CardTitle>{title}</CardTitle>
          <p className="text-xs text-muted">{summary}</p>
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
      <CardContent>
        <div className="relative h-56">
          <ResponsiveContainer width="100%" height="100%">
            {byModel ? (
              <LineChart
                data={data}
                margin={{ top: 4, right: 8, bottom: 0, left: 0 }}
                onMouseMove={trendHover.onChartMouseMove}
                onMouseLeave={trendHover.onChartMouseLeave}
              >
                {axis}
                {series.map((item) => (
                  <Line
                    key={item.id}
                    type="monotone"
                    dataKey={item.id}
                    name={item.name}
                    stroke={item.color}
                    strokeWidth={1.5}
                    dot={false}
                    isAnimationActive={false}
                    activeDot={{ r: 3, strokeWidth: 0 }}
                  />
                ))}
              </LineChart>
            ) : (
              <AreaChart
                data={data}
                margin={{ top: 4, right: 8, bottom: 0, left: 0 }}
                onMouseMove={trendHover.onChartMouseMove}
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
          {trendHover.tip ? (
            <UsageTrendTooltipCard
              label={trendHover.tip.label}
              items={trendHover.tip.items}
              onMouseEnter={trendHover.onTipMouseEnter}
              onMouseLeave={trendHover.onTipMouseLeave}
            />
          ) : null}
        </div>
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
