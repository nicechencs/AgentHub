import { AgentDot } from '@/components/shared/AgentDot';
import { Badge } from '@/components/ui/badge';
import { Skeleton } from '@/components/ui/skeleton';
import { agentDisplayName } from '@/config/agents';
import type { AgentId } from '@/lib/types';
import { cn } from '@/lib/utils';
import { adapterTargetBadge, type AdapterTargetAnalysisState } from './adapter-view-model';

/**
 * Target panorama: every catalog Agent as a compact selectable card with the
 * analyzed route conclusion. Order is the fixed catalog order (never re-sorted
 * by state). Unconfigurable targets stay visible but disabled, which is a
 * machine-state, not a compatibility conclusion.
 */
export function AdapterTargetGrid({
  agentIds,
  configurableIds,
  analyses,
  selectedAgentId,
  onSelect,
  onRetry,
}: {
  agentIds: readonly AgentId[];
  configurableIds: ReadonlySet<AgentId>;
  analyses: Partial<Record<AgentId, AdapterTargetAnalysisState>>;
  selectedAgentId: AgentId | '';
  onSelect: (agentId: AgentId) => void;
  onRetry: (agentId: AgentId) => void;
}) {
  return (
    <div role="listbox" aria-label="目标 Agent" className="grid grid-cols-2 gap-2 sm:grid-cols-3">
      {agentIds.map((agentId) => {
        const configurable = configurableIds.has(agentId);
        const state: AdapterTargetAnalysisState = configurable
          ? analyses[agentId] ?? { kind: 'loading' }
          : { kind: 'unconfigurable' };
        const selected = agentId === selectedAgentId;
        return (
          <button
            key={agentId}
            type="button"
            role="option"
            aria-selected={selected}
            disabled={!configurable}
            title={targetCardHint(state)}
            className={cn(
              'flex min-w-0 flex-col gap-1.5 rounded-card border p-2.5 text-left transition-colors',
              selected
                ? 'border-border-strong bg-active'
                : 'border-border bg-panel hover:bg-hover/50',
              !configurable && 'cursor-not-allowed opacity-55 hover:bg-panel',
            )}
            onClick={() => {
              onSelect(agentId);
              if (state.kind === 'error') onRetry(agentId);
            }}
          >
            <span className="flex min-w-0 items-center gap-1.5 text-sm font-medium">
              <AgentDot agentId={agentId} size="sm" title={null} />
              <span className="truncate">{agentDisplayName(agentId)}</span>
            </span>
            <TargetConclusion state={state} />
          </button>
        );
      })}
    </div>
  );
}

/** Native hover hint; the full explanation still lives in the preview pane. */
function targetCardHint(state: AdapterTargetAnalysisState): string | undefined {
  if (state.kind === 'unconfigurable') return '未安装或不可配置';
  if (state.kind === 'ready' && state.analysis.reason.trim()) return state.analysis.reason;
  return undefined;
}

function TargetConclusion({ state }: { state: AdapterTargetAnalysisState }) {
  if (state.kind === 'unconfigurable') {
    return <span className="text-xs text-muted">未安装或不可配置</span>;
  }
  if (state.kind === 'loading') {
    return <Skeleton className="h-4 w-16" aria-label="分析中" />;
  }
  if (state.kind === 'error') {
    return <span className="text-xs text-warning">分析失败 · 点击重试</span>;
  }
  const badge = adapterTargetBadge(state.analysis);
  return (
    <span>
      <Badge variant={badge.variant}>{badge.label}</Badge>
    </span>
  );
}
