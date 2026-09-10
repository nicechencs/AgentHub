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
import type { McpInventory, McpServerEntry, McpSourceFile } from '@/lib/backend/contracts/mcp-types';
import type { AgentKey } from '@/lib/types';
import { AgentDot } from '@/components/shared/AgentDot';
import { OpenDirButton } from '@/components/shared/OpenDirButton';
import { TruncateTip } from '@/components/ui/tooltip';
import { groupMcpServersByAgentAndFile } from './group-servers';
import { McpServerTable } from './McpServerTable';
import { visibleMcpSources } from './mcp-sources';

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

  const sources = useMemo(() => {
    const visible = filterByPageVisibleAgent(
      visibleMcpSources(data?.sources),
      (file) => file.agent,
      hiddenIds,
      installedIds,
      !agentsLoading,
    );
    if (filterAgent === 'all') return visible;
    return visible.filter((file) => file.agent === filterAgent);
  }, [data, filterAgent, hiddenIds, installedIds, agentsLoading]);

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
        ) : servers.length === 0 && sources.length > 0 ? (
          <McpSourceEmpty
            sources={sources}
            showAgent={filterAgent === 'all'}
            onLocate={locateSource}
          />
        ) : servers.length === 0 ? (
          <EmptyState
            icon={Plug}
            title={t('mcp.empty.title')}
            description={t('mcp.empty.oneLiner')}
            actionLabel={t('mcp.empty.addServer')}
            onAction={() => {
              const path = sources[0]?.path;
              if (path) {
                void locateSource(path);
                return;
              }
              toast({
                title: t('mcp.empty.addServer'),
                description: t('mcp.empty.addServerHint'),
              });
            }}
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

function McpSourceEmpty({
  sources,
  showAgent,
  onLocate,
}: {
  sources: McpSourceFile[];
  showAgent: boolean;
  onLocate: (path: string) => void;
}) {
  const { t } = useI18n();
  return (
    <div className="space-y-2">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <p className="text-body text-secondary">{t('mcp.empty.oneLiner')}</p>
        <Button
          size="sm"
          onClick={() => {
            const path = sources[0]?.path;
            if (path) onLocate(path);
          }}
        >
          {t('mcp.empty.addServer')}
        </Button>
      </div>
      {sources.map((file) => (
        <div
          key={`${file.agent}:${file.path}`}
          className="flex items-center justify-between gap-3 rounded-card border border-border bg-panel px-3 py-2"
        >
          <div className="min-w-0">
            <p className="flex min-w-0 items-center gap-2 truncate text-body font-medium">
              {showAgent ? (
                <>
                  <AgentDot agentId={file.agent} size="sm" title={null} />
                  <span className="shrink-0">{agentName(file.agent)}</span>
                  <span className="text-muted">·</span>
                </>
              ) : null}
              <TruncateTip className="truncate" text={file.label} />
            </p>
            <TruncateTip
              className={file.error ? 'text-meta text-danger' : 'text-meta text-muted'}
              text={
                file.error?.trim()
                  ? file.error
                  : file.readable
                    ? t('mcp.empty.sourceEmpty')
                    : t('mcp.empty.sourceUnreadable')
              }
            />
          </div>
          <OpenDirButton labeled title={file.path} onClick={() => onLocate(file.path)} />
        </div>
      ))}
    </div>
  );
}
