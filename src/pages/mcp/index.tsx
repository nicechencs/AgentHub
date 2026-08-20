import { useCallback, useEffect, useMemo, useState } from 'react';
import { Loader2, Plug, RefreshCw } from 'lucide-react';
import { AgentTabStrip, type AgentTabId } from '@/components/layout/AgentTabStrip';
import { PageHeader } from '@/components/layout/PageHeader';
import { PageSection } from '@/components/layout/PageSection';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { EmptyState } from '@/components/shared/EmptyState';
import { ErrorState } from '@/components/shared/ErrorState';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
import { ListSkeleton } from '@/components/ui/skeleton';
import { useToast } from '@/components/ui/toast';
import { resolveAgentMeta, agentDisplayName, type AgentMeta } from '@/config/agents';
import { useInstalledAgents } from '@/lib/hooks/useInstalledAgents';
import { listMcpInventory } from '@/lib/api/mcp';
import { openPathInFileManager } from '@/lib/api/skill';
import type { McpInventory, McpServerEntry } from '@/lib/backend/contracts/mcp-types';
import type { AgentId } from '@/lib/types';
import { groupMcpServersByAgentAndFile } from './group-servers';
import { McpServerTable } from './McpServerTable';

function agentName(id: AgentId): string {
  return agentDisplayName(id);
}

export default function McpPage() {
  const { t } = useI18n();
  const { toast } = useToast();
  const { hiddenIds } = useInstalledAgents();
  const hiddenSet = useMemo(() => new Set(hiddenIds), [hiddenIds]);
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

  const filterAgents = useMemo(() => {
    if (!data) return [] as AgentMeta[];
    const ids = new Set<AgentId>();
    for (const s of data.servers) ids.add(s.agent);
    return [...ids]
      .filter((id) => !hiddenSet.has(id))
      .sort((a, b) => a.localeCompare(b))
      .map((id) => resolveAgentMeta(id));
  }, [data, hiddenSet]);

  useEffect(() => {
    if (filterAgent !== 'all' && hiddenSet.has(filterAgent)) {
      setFilterAgent('all');
    }
  }, [filterAgent, hiddenSet]);

  const agentCounts = useMemo(() => {
    const visibleServers = data?.servers.filter((s) => !hiddenSet.has(s.agent)) ?? [];
    const counts: Partial<Record<AgentTabId, number>> = {
      all: visibleServers.length,
    };
    for (const s of visibleServers) {
      counts[s.agent] = (counts[s.agent] ?? 0) + 1;
    }
    return counts;
  }, [data, hiddenSet]);

  const servers = useMemo(() => {
    if (!data) return [] as McpServerEntry[];
    const visible = data.servers.filter((s) => !hiddenSet.has(s.agent));
    if (filterAgent === 'all') return visible;
    return visible.filter((s) => s.agent === filterAgent);
  }, [data, filterAgent, hiddenSet]);

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
        descriptionTip={t('mcp.page.descriptionTip')}
        actions={
          <Button
            size="sm"
            variant="secondary"
            disabled={loading}
            onClick={() => void load()}
            className="gap-1.5"
          >
            {loading ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <RefreshCw className="h-3.5 w-3.5" />}
            {t('mcp.page.refresh')}
          </Button>
        }
      />

      {loading && !data ? (
        <ListSkeleton rows={5} />
      ) : error && !data ? (
        <ErrorState error={error} onRetry={() => void load()} />
      ) : (
        <>
          <div className={pageRhythm.chrome}>
            <AgentTabStrip
              showAll
              allLabel={t('kind.all')}
              value={filterAgent}
              onChange={setFilterAgent}
              agents={filterAgents}
              counts={agentCounts}
              countMode="defined"
              countTitle={(id, n) =>
                id === 'all'
                  ? t('mcp.page.countAll', { n })
                  : t('mcp.page.countAgent', { name: agentName(id), n })
              }
              emptyLabel={t('mcp.page.emptyTabs')}
              aria-label={t('mcp.page.filterAria')}
            />
          </div>

          <PageSection first>
            {servers.length === 0 ? (
              <EmptyState
                icon={Plug}
                title={t('mcp.empty.title')}
                description={
                  filterAgent === 'all'
                    ? t('mcp.empty.all')
                    : t('mcp.empty.agent', { name: agentName(filterAgent) })
                }
                actionLabel={t('mcp.empty.refresh')}
                onAction={() => void load()}
              />
            ) : (
              <McpServerTable
                groups={agentGroups}
                showAgent={filterAgent === 'all'}
                onLocate={locateSource}
              />
            )}
          </PageSection>
        </>
      )}
    </div>
  );
}
