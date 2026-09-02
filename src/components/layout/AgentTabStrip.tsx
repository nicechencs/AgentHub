import type { ReactNode } from 'react';
import { AgentDot } from '@/components/shared/AgentDot';
import { Hint } from '@/components/ui/tooltip';
import {
  segmentedCountClass,
  segmentedItemClass,
  segmentedTrackClass,
  type SegmentedSize,
} from '@/components/ui/segmented-styles';
import { useI18n } from '@/components/shared/LanguageProvider';
import { AGENTS, type AgentMeta } from '@/config/agents';
import type { AgentKey } from '@/lib/types';
import { cn } from '@/lib/utils';

/** 页内 agent 切换条的取值；`all` 仅在 `showAll` 时出现。 */
export type AgentTabId = AgentKey | 'all';

/**
 * 普通数量何时展示：
 * - `positive`：仅 n>0（连接池条数）
 * - `defined`：有值就显示，含 0（项目计数加载后）
 * - `always`：始终显示（缺省当 0）
 */
export type AgentTabCountMode = 'positive' | 'defined' | 'always';

type AgentTabStripBase = {
  disabled?: AgentKey[];
  disabledReason?: string;
  /** 默认全量 AGENTS；非 Agents 页请传已安装子集 */
  agents?: readonly AgentMeta[];
  emptyLabel?: string;
  /** 固定 md；保留 prop 仅兼容，页面不应再传 sm */
  size?: SegmentedSize;
  /**
   * 普通数量角标（统一 segmentedCountClass）。
   * 与 renderEnd 可并存：先 renderEnd，再 count。
   */
  counts?: Partial<Record<AgentTabId, number | undefined | null>>;
  /** 默认 positive */
  countMode?: AgentTabCountMode;
  /** 并入 Tab 的 Hint，如 `${n} 条连接`；不要再写原生 title */
  countTitle?: (id: AgentTabId, n: number) => string;
  /**
   * 非普通计数的尾部（生效绿点、琥珀行动角标等）。
   * 不要在这里再画一遍明文数字，请用 counts。
   */
  renderEnd?: (id: AgentTabId) => ReactNode;
  className?: string;
  'aria-label'?: string;
};

export type AgentTabStripProps =
  | (AgentTabStripBase & {
      showAll?: false;
      value: AgentKey;
      onChange: (id: AgentKey) => void;
      allLabel?: never;
    })
  | (AgentTabStripBase & {
      showAll: true;
      allLabel?: string;
      value: AgentTabId;
      onChange: (id: AgentTabId) => void;
    });

function resolveCountDisplay(
  raw: number | undefined | null,
  mode: AgentTabCountMode,
): number | null {
  if (mode === 'always') return raw ?? 0;
  if (raw == null || Number.isNaN(raw)) return null;
  if (mode === 'positive') return raw > 0 ? raw : null;
  // defined
  return raw;
}

/**
 * 页内 Agent 切换条（全站唯一）：灰轨 + 白底抬起 + 品牌圆点。
 * 有的页只切换 Agent；有的页多带 counts；特例状态走 renderEnd。
 */
export function AgentTabStrip(props: AgentTabStripProps) {
  const { t } = useI18n();
  const {
    value,
    onChange,
    disabled,
    disabledReason,
    agents,
    emptyLabel = t('dashboard.overview.emptyTitle'),
    size = 'md',
    counts,
    countMode = 'positive',
    countTitle,
    renderEnd,
    className,
    'aria-label': ariaLabel = t('nav.agentTabs'),
  } = props;
  const showAll = props.showAll === true;
  const allLabel = showAll ? (props.allLabel ?? t('kind.all')) : t('kind.all');

  const list = agents ?? AGENTS;
  if (list.length === 0 && !showAll) {
    return (
      <div className="rounded-card bg-hover px-3 py-1.5 text-sm text-muted">{emptyLabel}</div>
    );
  }

  const tabClass = (active: boolean, isDisabled: boolean) =>
    cn(segmentedItemClass(active, size, { disabled: isDisabled }), 'gap-1.5');

  const select = (id: AgentTabId) => {
    if (showAll) {
      (onChange as (id: AgentTabId) => void)(id);
    } else if (id !== 'all') {
      (onChange as (id: AgentKey) => void)(id);
    }
  };

  const countHint = (id: AgentTabId): string | undefined => {
    const n = resolveCountDisplay(counts?.[id], countMode);
    if (n == null || !countTitle) return undefined;
    return countTitle(id, n);
  };

  const endSlot = (id: AgentTabId) => {
    const extra = renderEnd?.(id);
    const n = resolveCountDisplay(counts?.[id], countMode);
    const countNode =
      n != null ? <span className={segmentedCountClass}>{n}</span> : null;
    if (!extra && !countNode) return null;
    if (extra && countNode) {
      return (
        <span className="inline-flex items-center gap-1">
          {extra}
          {countNode}
        </span>
      );
    }
    return extra ?? countNode;
  };

  return (
    <div
      role="tablist"
      aria-label={ariaLabel}
      className={cn(segmentedTrackClass, className)}
    >
      {showAll ? (
        <Hint label={[allLabel, countHint('all')].filter(Boolean).join(' · ')}>
          <button
            type="button"
            role="tab"
            aria-selected={value === 'all'}
            onClick={() => select('all')}
            className={tabClass(value === 'all', false)}
          >
            {allLabel}
            {endSlot('all')}
          </button>
        </Hint>
      ) : null}
      {list.map((meta) => {
        const isDisabled = disabled?.includes(meta.id) ?? false;
        const active = value === meta.id;
        const tip = isDisabled
          ? (disabledReason ?? t('nav.featureUnsupported', { name: meta.name }))
          : [meta.name, countHint(meta.id)].filter(Boolean).join(' · ');
        return (
          <Hint key={meta.id} label={tip}>
            <button
              type="button"
              role="tab"
              aria-selected={active}
              disabled={isDisabled}
              onClick={() => select(meta.id)}
              className={tabClass(active, isDisabled)}
            >
              <AgentDot agentId={meta.id} color={meta.color} size={size === 'sm' ? 'sm' : 'md'} />
              {meta.name.replace(' Code', '')}
              {endSlot(meta.id)}
            </button>
          </Hint>
        );
      })}
    </div>
  );
}
