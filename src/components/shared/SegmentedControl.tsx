import type { KeyboardEvent, ReactNode } from 'react';
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
  const onTabListKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return;
    const tabs = Array.from(
      event.currentTarget.querySelectorAll<HTMLButtonElement>('[role="tab"]:not(:disabled)'),
    );
    if (tabs.length === 0) return;
    const current = document.activeElement instanceof HTMLButtonElement
      ? tabs.indexOf(document.activeElement)
      : -1;
    const fallback = Math.max(0, tabs.findIndex((tab) => tab.getAttribute('aria-selected') === 'true'));
    const index = current >= 0 ? current : fallback;
    const nextIndex = event.key === 'Home'
      ? 0
      : event.key === 'End'
        ? tabs.length - 1
        : (index + (event.key === 'ArrowRight' ? 1 : -1) + tabs.length) % tabs.length;
    event.preventDefault();
    tabs[nextIndex]?.focus();
    tabs[nextIndex]?.click();
  };

  return (
    <div
      role="tablist"
      aria-label={ariaLabel}
      onKeyDown={onTabListKeyDown}
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
              tabIndex={active ? 0 : -1}
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
