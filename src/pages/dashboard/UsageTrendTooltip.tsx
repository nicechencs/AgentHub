import { useCallback, useRef, useState } from 'react';

import { tooltipSurfaceStyle } from '@/components/ui/tooltip';
import {
  formatTrendTooltipLabel,
  usageTrendTooltipItemsFromPayload,
  type UsageTrendTooltipItem,
  type UsageTrendTooltipPayloadEntry,
} from '@/lib/usage-trend';
import { fmtTokens } from '@/lib/utils';

/** Room for `999.9K` / `12.3M` on the left axis without clipping. */
export const USAGE_TREND_Y_AXIS_WIDTH = 64;

type ChartHoverState = {
  activeLabel?: unknown;
  activePayload?: readonly UsageTrendTooltipPayloadEntry[];
} | null;

export function useUsageTrendHover(
  resolveName?: (key: string) => string,
  options?: {
    formatValue?: (value: number) => string;
    extraFor?: (
      key: string,
      value: number,
      payload?: Record<string, unknown>,
    ) => string | undefined;
  },
) {
  const [tip, setTip] = useState<{ label: string; items: UsageTrendTooltipItem[] } | null>(
    null,
  );
  const pinnedRef = useRef(false);
  const leaveTimerRef = useRef<number | null>(null);
  const formatValue = options?.formatValue;
  const extraFor = options?.extraFor;

  const clearLeaveTimer = useCallback(() => {
    if (leaveTimerRef.current == null) return;
    window.clearTimeout(leaveTimerRef.current);
    leaveTimerRef.current = null;
  }, []);

  const onChartMouseMove = useCallback(
    (state: ChartHoverState) => {
      if (pinnedRef.current) return;
      const payload = state?.activePayload;
      const items = usageTrendTooltipItemsFromPayload(payload, resolveName).map((item) => {
        const entry = payload?.find((row) => String(row.dataKey ?? row.name ?? '') === item.key);
        const data =
          entry?.payload && typeof entry.payload === 'object'
            ? (entry.payload as Record<string, unknown>)
            : undefined;
        return {
          ...item,
          formatted: formatValue?.(item.tokens),
          extra: extraFor?.(item.key, item.tokens, data),
        };
      });
      if (!items.length) {
        setTip(null);
        return;
      }
      setTip({ label: String(state?.activeLabel ?? ''), items });
    },
    [extraFor, formatValue, resolveName],
  );

  const onChartMouseLeave = useCallback(() => {
    clearLeaveTimer();
    leaveTimerRef.current = window.setTimeout(() => {
      if (!pinnedRef.current) setTip(null);
    }, 80);
  }, [clearLeaveTimer]);

  const onTipMouseEnter = useCallback(() => {
    pinnedRef.current = true;
    clearLeaveTimer();
  }, [clearLeaveTimer]);

  const onTipMouseLeave = useCallback(() => {
    pinnedRef.current = false;
    setTip(null);
  }, []);

  return { tip, onChartMouseMove, onChartMouseLeave, onTipMouseEnter, onTipMouseLeave };
}

export function UsageTrendTooltipCard({
  label,
  items,
  onMouseEnter,
  onMouseLeave,
}: {
  label: string;
  items: readonly UsageTrendTooltipItem[];
  onMouseEnter?: () => void;
  onMouseLeave?: () => void;
}) {
  if (!items.length) return null;
  return (
    <div
      className="absolute right-1 top-1 z-10"
      style={{ ...tooltipSurfaceStyle(), pointerEvents: 'auto', overscrollBehavior: 'contain' }}
      onMouseEnter={onMouseEnter}
      onMouseLeave={onMouseLeave}
    >
      <div className="text-secondary">{formatTrendTooltipLabel(label)}</div>
      <ul className="m-0 list-none p-0">
        {items.map((item) => (
          <li
            key={item.key}
            className="flex justify-between gap-3 pt-1"
            style={item.color ? { color: item.color } : undefined}
          >
            <span>{item.name}</span>
            <span className="flex gap-2 tabular-nums">
              <span>{item.formatted ?? fmtTokens(item.tokens)}</span>
              {item.extra ? <span className="text-muted">{item.extra}</span> : null}
            </span>
          </li>
        ))}
      </ul>
    </div>
  );
}
