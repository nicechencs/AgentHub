import { useCallback, useEffect, useMemo, useState } from 'react';
import { Plug } from 'lucide-react';
import { AgentTabStrip, type AgentTabId } from '@/components/layout/AgentTabStrip';
import { PageHeader } from '@/components/layout/PageHeader';
import { PageSection } from '@/components/layout/PageSection';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { EmptyState } from '@/components/shared/EmptyState';
import { ErrorState } from '@/components/shared/ErrorState';
import { useI18n } from '@/components/shared/LanguageProvider';
import { PageRefreshButton } from '@/components/shared/PageRefreshButton';
import { Button } from '@/components/ui/button';
import { TableSkeleton } from '@/components/ui/skeleton';
import { useToast } from '@/components/ui/toast';
import { agentDisplayName } from '@/config/agents';
import { filterByPageVisibleAgent } from '@/lib/agent-visibility';
import { useInstalledAgents } from '@/lib/hooks/useInstalledAgents';
import { listMcpInventory } from '@/lib/api/mcp';
import { openPathInFileManager } from '@/lib/api/skill';
import type { McpInventory, McpServerEntry } from '@/lib/backend/contracts/mcp-types';
import type { AgentKey } from '@/lib/types';
import { groupMcpServersByAgentAndFile } from './group-servers';
import { McpServerTable } from './McpServerTable';

function agentName(id: AgentKey): string {
  return agentDisplayName(id);
}

export default function McpPage() {
  const { t } = useI18n();
  const { toast } = useToast();
  const { hiddenIds, installedIds, installedAgents, loading: agentsLoading } = useInstalledAgents();
  const [data, setData] = useState<McpInventory | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<Error | string | null>(null);
  const [filterAgent, setFilterAgent] = useState<AgentTabId>('all');

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const inv = await listMcpInventory();
      setData(inv);
    } catch (e) {
      setError(e instanceof Error ? e : String(e));
      setData(null);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    if (filterAgent === 'all') return;
    if (!installedAgents.some((a) => a.id === filterAgent)) {
      setFilterAgent('all');
    }
  }, [filterAgent, installedAgents]);

  const agentCounts = useMemo(() => {
    const visibleServers = data
      ? filterByPageVisibleAgent(
          data.servers,
          (s) => s.agent,
          hiddenIds,
          installedIds,
          !agentsLoading,
        )
      : [];
    const counts: Partial<Record<AgentTabId, number>> = {
      all: visibleServers.length,
    };
    for (const a of installedAgents) counts[a.id] = 0;
    for (const s of visibleServers) {
      counts[s.agent] = (counts[s.agent] ?? 0) + 1;
    }
    return counts;
  }, [data, hiddenIds, installedIds, agentsLoading, installedAgents]);

  const servers = useMemo(() => {
    if (!data) return [] as McpServerEntry[];
    const visible = filterByPageVisibleAgent(
      data.servers,
      (s) => s.agent,
      hiddenIds,
      installedIds,
      !agentsLoading,
    );
    if (filterAgent === 'all') return visible;
    return visible.filter((s) => s.agent === filterAgent);
  }, [data, filterAgent, hiddenIds, installedIds, agentsLoading]);

  const agentGroups = useMemo(() => groupMcpServersByAgentAndFile(servers), [servers]);

  async function locateSource(path: string) {
    try {
      await openPathInFileManager(path);
    } catch (e) {
      toast({
        title: t('mcp.toast.cannotOpenDir'),
        description: e instanceof Error ? e.message : String(e),
        variant: 'danger',
      });
    }
  }

  return (
    <div>
      <PageHeader
        title={t('mcp.page.title')}
        description={t('mcp.page.description')}
        descriptionTip={t('mcp.page.descriptionTip', { next: t('mcp.page.nextStep') })}
      />

      <div className={pageRhythm.chromeRow} data-help="page-chrome">
        <AgentTabStrip
          showAll
          allLabel={t('kind.all')}
          value={filterAgent}
          onChange={setFilterAgent}
          agents={installedAgents}
          counts={data ? agentCounts : undefined}
          countMode="defined"
          countTitle={(id, n) =>
            id === 'all'
              ? t('mcp.page.countAll', { n })
              : t('mcp.page.countAgent', { name: agentName(id), n })
          }
          emptyLabel={t('mcp.page.emptyTabs')}
          aria-label={t('mcp.page.filterAria')}
        />
        <div className={pageRhythm.chromeActions}>
          <PageRefreshButton
            loading={loading}
            onClick={() => void load()}
            label={t('mcp.page.refresh')}
          />
        </div>
      </div>

      <PageSection first data-help="mcp-list">
        {loading && !data ? (
          <TableSkeleton rows={6} cols={4} />
        ) : error && !data ? (
          <ErrorState error={error} onRetry={() => void load()} />
        ) : servers.length === 0 ? (
          <EmptyState
            icon={Plug}
            title={t('mcp.empty.title')}
            description={
              filterAgent === 'all'
                ? t('mcp.page.empty', { next: t('mcp.page.nextStep') })
                : t('mcp.page.emptyAgent', { name: agentName(filterAgent) })
            }
            action={
              <Button
                size="sm"
                variant="outline"
                className="mt-2"
                onClick={() => void load()}
              >
                {t('mcp.empty.refresh')}
              </Button>
            }
          />
        ) : (
          <McpServerTable
            groups={agentGroups}
            showAgent={filterAgent === 'all'}
            onLocate={locateSource}
          />
        )}
      </PageSection>
    </div>
  );
}
