import type { ReactNode } from 'react';
import { Hint } from '@/components/ui/tooltip';
import {
  segmentedCountClass,
  segmentedItemClass,
  segmentedTrackClass,
  type SegmentedSize,
} from '@/components/ui/segmented-styles';
import { cn } from '@/lib/utils';

export interface SegmentedOption<T extends string = string> {
  value: T;
  label: ReactNode;
  /** 普通数量角标（始终展示含 0）；页内筛选默认带 count */
  count?: number;
  disabled?: boolean;
  title?: string;
}

/**
 * 页内列表筛选分段（默认 sm）。
 * 页级导航用 `Tabs`；Agent 切换用 `AgentTabStrip`。见 segmented-styles 约定。
 */
export function SegmentedControl<T extends string = string>({
  value,
  onChange,
  options,
  size = 'sm',
  className,
  'aria-label': ariaLabel,
}: {
  value: T;
  onChange: (value: T) => void;
  options: SegmentedOption<T>[];
  size?: SegmentedSize;
  className?: string;
  'aria-label'?: string;
}) {
  return (
    <div
      role="tablist"
      aria-label={ariaLabel}
      className={cn(segmentedTrackClass, className)}
    >
      {options.map((opt) => {
        const active = value === opt.value;
        return (
          <Hint key={opt.value} label={opt.title}>
            <button
              type="button"
              role="tab"
              aria-selected={active}
              disabled={opt.disabled}
              onClick={() => onChange(opt.value)}
              className={cn(
                segmentedItemClass(active, size, { disabled: opt.disabled }),
                opt.count != null && 'gap-1.5',
              )}
            >
              {opt.label}
              {opt.count != null ? (
                <span className={segmentedCountClass}>{opt.count}</span>
              ) : null}
            </button>
          </Hint>
        );
      })}
    </div>
  );
}
