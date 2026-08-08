import { useNavigate } from 'react-router-dom';
import { Bot } from 'lucide-react';

import { AgentLogo } from '@/components/shared/AgentLogo';
import { EmptyState } from '@/components/shared/EmptyState';
import { StatusDot } from '@/components/shared/StatusDot';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import { Skeleton } from '@/components/ui/skeleton';
import { Tip } from '@/components/ui/tooltip';
import { AGENTS } from '@/config/agents';
import type { AgentStatus } from '@/lib/types';
import { cn } from '@/lib/utils';

import {
  AGENT_OVERVIEW_GRID,
  mergeAgentsInOrder,
  summarizeAgentOverview,
} from './agentOverviewModel';

export function AgentOverview({ agents }: { agents: AgentStatus[] }) {
  const navigate = useNavigate();
  // Dashboard 只展示已安装 Agent；未安装的去 Agents 页安装。
  const installedMetas = AGENTS.filter((m) =>
    agents.some((a) => a.agentId === m.id && a.installed),
  );
  const installedStatuses = agents.filter((a) => a.installed);
  const { summaryText } = summarizeAgentOverview(installedMetas, installedStatuses);
  const cards = mergeAgentsInOrder(installedMetas, installedStatuses);

  return (
    <div>
      <div className="mb-3 flex items-center gap-2 text-sm">
        <span>
          <span className="font-medium text-primary">Agent 总览</span>
          <span className="text-muted"> {summaryText}</span>
        </span>
        <Button
          variant="ghost"
          size="sm"
          className="ml-auto h-6 px-2 text-xs"
          onClick={() => navigate('/agents')}
        >
          管理
        </Button>
      </div>

      {cards.length === 0 ? (
        <EmptyState
          icon={Bot}
          title="尚未安装 Agent"
          description="安装后可在此查看状态"
          actionLabel="去安装"
          onAction={() => navigate('/agents')}
        />
      ) : (
        <div className={AGENT_OVERVIEW_GRID}>
          {cards.map(({ meta, view }) => {
            const go = () => navigate(view.target);
            return (
              <Card
                key={meta.id}
                role="button"
                tabIndex={0}
                aria-label={view.ariaLabel}
                onClick={go}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' || e.key === ' ') {
                    e.preventDefault();
                    go();
                  }
                }}
                className={cn(
                  'cursor-pointer p-3 transition-colors hover:border-accent/40 hover:bg-hover/40',
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
                        className="min-w-0 truncate text-2xs font-normal text-muted"
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
                <Tip
                  className={cn('mt-1.5 truncate text-xs', view.metaClass)}
                  label={view.titleFull}
                >
                  {view.metaText}
                </Tip>
              </Card>
            );
          })}
        </div>
      )}
    </div>
  );
}

export function AgentOverviewSkeleton() {
  return (
    <div>
      <div className="mb-3 flex items-center gap-2">
        <Skeleton className="h-3 w-40" />
        <Skeleton className="ml-auto h-6 w-10" />
      </div>
      <div className={AGENT_OVERVIEW_GRID}>
        {AGENTS.map((meta) => (
          <Card key={meta.id} className="p-3">
            <div className="flex items-center gap-2">
              <Skeleton className="h-6 w-6 shrink-0 rounded-full" />
              <Skeleton className="h-4 w-20" />
              <Skeleton className="ml-auto h-2 w-2 shrink-0 rounded-full" />
            </div>
            <Skeleton className="mt-1.5 h-3 w-28" />
          </Card>
        ))}
      </div>
    </div>
  );
}
