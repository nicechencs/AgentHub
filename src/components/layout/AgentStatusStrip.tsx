import { NavLink } from 'react-router-dom';
import { AgentDot } from '@/components/shared/AgentDot';
import { AGENTS, type AgentMeta } from '@/config/agents';
import { useAgentStatusesOptional } from '@/app/runtime';
import type { AgentStatus } from '@/lib/types';
import {
  agentHasCatalogUpdate,
  sidebarInstallStats,
} from '@/components/layout/sidebar-stats';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { cn } from '@/lib/utils';
import { useI18n } from '@/components/shared/LanguageProvider';
import { useStoredIdOrder } from '@/components/shared/use-stored-id-order';
import { StorageKey } from '@/lib/ui-preferences';

function agentDotLabel(
  meta: AgentMeta,
  status: AgentStatus | undefined,
  hasUpdate: boolean,
  upgradeable: string,
): string {
  const ver = status?.version ? ` v${status.version}` : '';
  const up = hasUpdate ? upgradeable : '';
  return `${meta.name}${ver}${up}`;
}

/** 底栏左侧：已安装 Agent 圆点，点进去是 Agent 页。不是当前对话的 Agent。 */
export function AgentStatusStrip() {
  const { t } = useI18n();
  const { statuses: agents } = useAgentStatusesOptional();
  const { stored: agentCatalogOrder } = useStoredIdOrder(StorageKey.agentsCatalogOrder);
  const stats = sidebarInstallStats(AGENTS, agents, agentCatalogOrder);
  const fractionLabel = t('nav.agentsInstalled', {
    installed: stats.installedCount,
    total: stats.visibleTotal,
  });

  return (
    <NavLink
      to="/agents"
      className={cn(pageRhythm.statusBarItem, 'min-w-0')}
      aria-label={fractionLabel}
      title={fractionLabel}
    >
      <span className="flex min-w-0 flex-wrap items-center gap-1">
        {stats.orderedInstalledMetas.map((meta) => {
          const status = agents.find((row) => row.agentId === meta.id);
          const hasUpdate = agentHasCatalogUpdate(status);
          return (
            <AgentDot
              key={meta.id}
              agentId={meta.id}
              color={meta.color}
              title={agentDotLabel(
                meta,
                status,
                hasUpdate,
                t('nav.upgradeable', { version: status?.latestVersion ?? '' }),
              )}
              className={cn(hasUpdate && 'ring-2 ring-warning')}
            />
          );
        })}
        {stats.installedCount === 0 ? (
          <span className="text-muted">{t('nav.noAgentInstalled')}</span>
        ) : (
          <span className="shrink-0 tabular-nums text-muted">
            {stats.installedCount}/{stats.visibleTotal}
          </span>
        )}
      </span>
    </NavLink>
  );
}
