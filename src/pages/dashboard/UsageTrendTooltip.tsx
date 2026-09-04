import { useCallback, useLayoutEffect, useRef, useState } from 'react';

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

const TIP_PAD = 4;
const TIP_GAP = 12;
const TIP_MAX_WIDTH_PX = 280;

export type ChartHoverState = {
  activeLabel?: unknown;
  activePayload?: readonly UsageTrendTooltipPayloadEntry[];
  activeCoordinate?: { x?: number; y?: number };
  chartX?: number;
  chartY?: number;
} | null;

export type UsageTrendHoverTip = {
  label: string;
  items: UsageTrendTooltipItem[];
  dailyTotal?: number;
  cumulativeTotal?: number;
  x: number;
  y: number;
  containerWidth: number;
  containerHeight: number;
};

export function usageTrendHoverPoint(
  state: ChartHoverState,
): { x: number; y: number } | null {
  if (!state) return null;
  const coord = state.activeCoordinate;
  const x =
    typeof coord?.x === 'number'
      ? coord.x
      : typeof state.chartX === 'number'
        ? state.chartX
        : NaN;
  const y =
    typeof coord?.y === 'number'
      ? coord.y
      : typeof state.chartY === 'number'
        ? state.chartY
        : NaN;
  if (!Number.isFinite(x) || !Number.isFinite(y)) return null;
  return { x, y };
}

/** Keep the full card inside the plot; never shrink it to the leftover strip. */
export function usageTrendTipOffset(
  x: number,
  y: number,
  containerWidth: number,
  containerHeight: number,
  tipWidth: number,
  tipHeight: number,
  gap = TIP_GAP,
): { left: number; top: number } {
  const cw = Math.max(0, containerWidth);
  const ch = Math.max(0, containerHeight);
  const tw = cw > 0 ? Math.min(tipWidth, Math.max(0, cw - TIP_PAD * 2)) : tipWidth;
  const th = ch > 0 ? Math.min(tipHeight, Math.max(0, ch - TIP_PAD * 2)) : tipHeight;
  const maxLeft = Math.max(TIP_PAD, cw - tw - TIP_PAD);
  const maxTop = Math.max(TIP_PAD, ch - th - TIP_PAD);
  let left = x + gap;
  if (cw > 0 && left + tw > cw - TIP_PAD) left = x - gap - tw;
  if (cw > 0) left = Math.min(Math.max(TIP_PAD, left), maxLeft);
  let top = y - gap;
  if (ch > 0 && top + th > ch - TIP_PAD) top = y + gap;
  if (ch > 0) top = Math.min(Math.max(TIP_PAD, top), maxTop);
  return { left, top };
}

export function useUsageTrendHover(
  resolveName?: (key: string) => string,
  options?: {
    formatValue?: (value: number) => string;
    extraFor?: (
      key: string,
      value: number,
      payload?: Record<string, unknown>,
    ) => string | undefined;
    buildTip?: (state: ChartHoverState) => Omit<
      UsageTrendHoverTip,
      'x' | 'y' | 'containerWidth' | 'containerHeight'
    > | null;
  },
) {
  const [tip, setTip] = useState<UsageTrendHoverTip | null>(null);
  const pinnedRef = useRef(false);
  const leaveTimerRef = useRef<number | null>(null);
  const formatValue = options?.formatValue;
  const extraFor = options?.extraFor;
  const buildTip = options?.buildTip;

  const clearLeaveTimer = useCallback(() => {
    if (leaveTimerRef.current == null) return;
    window.clearTimeout(leaveTimerRef.current);
    leaveTimerRef.current = null;
  }, []);

  const onChartMouseMove = useCallback(
    (state: ChartHoverState, container?: HTMLElement | null) => {
      if (pinnedRef.current) return;
      const point = usageTrendHoverPoint(state);
      if (!point) {
        setTip(null);
        return;
      }
      const box = {
        width: container?.clientWidth ?? 0,
        height: container?.clientHeight ?? 0,
      };
      if (buildTip) {
        const next = buildTip(state);
        setTip(
          next && next.items.length
            ? {
                ...next,
                ...point,
                containerWidth: box.width,
                containerHeight: box.height,
              }
            : null,
        );
        return;
      }
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
      setTip({
        label: String(state?.activeLabel ?? ''),
        items,
        ...point,
        containerWidth: box.width,
        containerHeight: box.height,
      });
    },
    [buildTip, extraFor, formatValue, resolveName],
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
  dailyTotal,
  cumulativeTotal,
  dailyTotalLabel,
  cumulativeTotalLabel,
  x,
  y,
  containerWidth = 0,
  containerHeight = 0,
  onMouseEnter,
  onMouseLeave,
}: {
  label: string;
  items: readonly UsageTrendTooltipItem[];
  dailyTotal?: number;
  cumulativeTotal?: number;
  dailyTotalLabel?: string;
  cumulativeTotalLabel?: string;
  x: number;
  y: number;
  containerWidth?: number;
  containerHeight?: number;
  onMouseEnter?: () => void;
  onMouseLeave?: () => void;
}) {
  const nodeRef = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ width: TIP_MAX_WIDTH_PX, height: 80 });

  useLayoutEffect(() => {
    const node = nodeRef.current;
    if (!node) return;
    const rect = node.getBoundingClientRect();
    if (rect.width <= 0 || rect.height <= 0) return;
    setSize((prev) =>
      Math.abs(prev.width - rect.width) < 0.5 && Math.abs(prev.height - rect.height) < 0.5
        ? prev
        : { width: rect.width, height: rect.height },
    );
  }, [items, dailyTotal, cumulativeTotal, label]);

  if (!items.length) return null;
  const maxWidth =
    containerWidth > 0
      ? Math.min(TIP_MAX_WIDTH_PX, Math.max(160, containerWidth - TIP_PAD * 2))
      : TIP_MAX_WIDTH_PX;
  const pos = usageTrendTipOffset(
    x,
    y,
    containerWidth,
    containerHeight,
    size.width,
    size.height,
  );
  return (
    <div
      ref={nodeRef}
      className="absolute z-10"
      style={{
        ...tooltipSurfaceStyle(),
        left: pos.left,
        top: pos.top,
        width: 'max-content',
        maxWidth,
        minWidth: Math.min(160, maxWidth),
        pointerEvents: 'auto',
        overscrollBehavior: 'contain',
      }}
      onMouseEnter={onMouseEnter}
      onMouseLeave={onMouseLeave}
      onWheel={(event) => event.stopPropagation()}
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
              {item.share ? <span className="text-muted">{item.share}</span> : null}
              {item.extra ? <span className="text-muted">{item.extra}</span> : null}
            </span>
          </li>
        ))}
      </ul>
      {dailyTotal != null && dailyTotalLabel ? (
        <div className="mt-2 flex justify-between gap-3 border-t border-border pt-2 text-secondary">
          <span>{dailyTotalLabel}</span>
          <span className="tabular-nums">{fmtTokens(dailyTotal)}</span>
        </div>
      ) : null}
      {cumulativeTotal != null && cumulativeTotalLabel ? (
        <div className="flex justify-between gap-3 pt-1 text-secondary">
          <span>{cumulativeTotalLabel}</span>
          <span className="tabular-nums">{fmtTokens(cumulativeTotal)}</span>
        </div>
      ) : null}
    </div>
  );
}
