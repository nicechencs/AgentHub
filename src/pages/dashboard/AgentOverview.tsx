import { useNavigate } from 'react-router-dom';
import { Bot } from 'lucide-react';

import { useAgentCatalog } from '@/app/runtime';
import { AgentLogo } from '@/components/shared/AgentLogo';
import { EmptyState } from '@/components/shared/EmptyState';
import { useI18n } from '@/components/shared/LanguageProvider';
import { useStoredIdOrder } from '@/components/shared/use-stored-id-order';
import { StatusDot } from '@/components/shared/StatusDot';
import { Card } from '@/components/ui/card';
import { Skeleton } from '@/components/ui/skeleton';
import { Tip } from '@/components/ui/tooltip';
import { AGENTS } from '@/config/agents';
import { applyStoredAgentOrder } from '@/lib/agent-visibility';
import type { AgentKey, AgentStatus } from '@/lib/types';
import { StorageKey } from '@/lib/ui-preferences';
import { cn } from '@/lib/utils';

import {
  AGENT_OVERVIEW_GRID,
  installedOverviewScope,
  mergeAgentsInOrder,
  resolveAgentCardInteraction,
  type AgentCardBadgeInput,
} from './agentOverviewModel';

export type { AgentCardBadgeInput };

export interface AgentOverviewProps {
  agents: AgentStatus[];
  /** 未传时 connect 退化为 /connections?agent=X，不再打开总览弹窗 */
  onConnectRequest?: (agentId: AgentKey) => void;
  /** 当前正在用的授权；不传则只显示本机状态文案 */
  badgeInputs?: Readonly<Partial<Record<AgentKey, AgentCardBadgeInput>>>;
}

export function AgentOverview({
  agents,
  onConnectRequest,
  badgeInputs,
}: AgentOverviewProps) {
  const navigate = useNavigate();
  const { t } = useI18n();
  const catalog = useAgentCatalog();
  const { stored: agentCatalogOrder } = useStoredIdOrder(StorageKey.agentsCatalogOrder);
  const { metas: installedMetas, statuses: installedStatuses } = installedOverviewScope(
    catalog.hydrated ? AGENTS : [],
    agents,
  );
  const orderedMetas = applyStoredAgentOrder(
    installedMetas,
    (meta) => meta.id,
    agentCatalogOrder,
  );
  const cards = mergeAgentsInOrder(orderedMetas, installedStatuses, badgeInputs, t);

  return (
    <div>
      {cards.length === 0 ? (
        <EmptyState
          icon={Bot}
          title={t('dashboard.overview.emptyTitle')}
          description={t('dashboard.overview.emptyDesc')}
          actionLabel={t('dashboard.overview.emptyAction')}
          onAction={() => navigate('/agents')}
        />
      ) : (
        <div className={AGENT_OVERVIEW_GRID}>
          {cards.map(({ meta, view }) => {
            const interactive = view.action.kind !== 'none';
            const go = () => {
              const next = resolveAgentCardInteraction(view.action, meta.id, onConnectRequest);
              if (next.type === 'none') return;
              if (next.type === 'connect') {
                onConnectRequest?.(next.agentId);
                return;
              }
              navigate(next.to);
            };
            return (
              <Card
                key={meta.id}
                role={interactive ? 'button' : undefined}
                tabIndex={interactive ? 0 : undefined}
                aria-label={view.ariaLabel}
                aria-disabled={interactive ? undefined : true}
                onClick={interactive ? go : undefined}
                onKeyDown={interactive ? (e) => {
                  if (e.key === 'Enter' || e.key === ' ') {
                    e.preventDefault();
                    go();
                  }
                } : undefined}
                className={cn(
                  'p-3',
                  interactive && 'cursor-pointer transition-colors hover:border-accent/40 hover:bg-hover/40',
                  !interactive && 'cursor-default opacity-70',
                  view.missing && 'opacity-70',
                  view.envMissing && 'border-warning/40',
                )}
              >
                <div className="flex items-center gap-2">
                  <AgentLogo agentId={meta.id} size="sm" />
                  {/* 名称优先完整展示；版本只占剩余空间，过长时自己截断，不挤压名称 */}
                  <div className="flex min-w-0 flex-1 items-center gap-1.5 overflow-hidden">
                    <Tip className="shrink-0 text-sm font-medium" label={meta.name}>
                      {meta.name}
                    </Tip>
                    {view.versionText ? (
                      <Tip
                        className="min-w-0 truncate text-meta font-normal text-muted"
                        label={view.versionText}
                      >
                        {view.versionText}
                      </Tip>
                    ) : null}
                  </div>
                  <Tip className="shrink-0" label={view.statusDotTitle}>
                    <StatusDot status={view.authStatus} />
                  </Tip>
                </div>
                <div className="mt-1.5 flex min-w-0 items-center gap-1">
                  <Tip
                    className={cn('min-w-0 truncate text-xs', view.metaClass)}
                    label={view.titleFull}
                  >
                    {view.metaText}
                  </Tip>
                </div>
              </Card>
            );
          })}
        </div>
      )}
    </div>
  );
}

export function AgentOverviewSkeleton({ count }: { count: number }) {
  const n = Math.max(0, count);
  return (
    <div className={AGENT_OVERVIEW_GRID}>
      {Array.from({ length: n }).map((_, i) => (
        <Card key={i} className="p-3">
          <div className="flex items-center gap-2">
            <Skeleton className="h-6 w-6 shrink-0 rounded-mark" />
            <Skeleton className="h-4 w-20" />
            <Skeleton className="ml-auto h-2 w-2 shrink-0 rounded-full" />
          </div>
          <Skeleton className="mt-1.5 h-3 w-28" />
        </Card>
      ))}
    </div>
  );
}
