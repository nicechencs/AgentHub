import { useCallback, useEffect, useMemo, useState } from 'react';
import { Loader2, Puzzle, RefreshCw } from 'lucide-react';
import { AgentTabStrip, type AgentTabId } from '@/components/layout/AgentTabStrip';
import { PageHeader } from '@/components/layout/PageHeader';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { WorkbenchSplitPage } from '@/components/layout/SideSplit';
import { useSideSplit } from '@/components/layout/use-side-split';
import { EmptyState } from '@/components/shared/EmptyState';
import { ErrorState } from '@/components/shared/ErrorState';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
import { ListSkeleton } from '@/components/ui/skeleton';
import { useToast } from '@/components/ui/toast';
import { agentDisplayName } from '@/config/agents';
import { useInstalledAgents } from '@/lib/hooks/useInstalledAgents';
import { listPluginInventory } from '@/lib/api/plugins';
import { openPathInFileManager } from '@/lib/api/skill';
import type { PluginEntry, PluginInventory } from '@/lib/backend/contracts/plugin-types';
import type { AgentId } from '@/lib/types';
import { PluginDetailPanel } from './PluginDetailPanel';
import { PluginPackList } from './PluginPackList';

const PLUGINS_PREVIEW_WIDTH_KEY = 'agenthub.plugins.previewWidth';

function agentName(id: AgentId): string {
  return agentDisplayName(id);
}

export default function PluginsPage() {
  const { t } = useI18n();
  const { toast } = useToast();
  const { hiddenIds, installedAgents } = useInstalledAgents();
  const hiddenSet = useMemo(() => new Set(hiddenIds), [hiddenIds]);
  const [data, setData] = useState<PluginInventory | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<Error | string | null>(null);
  const [filterAgent, setFilterAgent] = useState<AgentTabId>('all');
  const inspect = useSideSplit<PluginEntry>({ storageKey: PLUGINS_PREVIEW_WIDTH_KEY });

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const inv = await listPluginInventory();
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

  const visiblePlugins = useMemo(() => {
    if (!data) return [] as PluginEntry[];
    return data.plugins.filter((p) => !hiddenSet.has(p.agent));
  }, [data, hiddenSet]);

  const plugins = useMemo(() => {
    if (filterAgent === 'all') return visiblePlugins;
    return visiblePlugins.filter((p) => p.agent === filterAgent);
  }, [filterAgent, visiblePlugins]);

  const agentCounts = useMemo(() => {
    const counts: Partial<Record<AgentTabId, number>> = { all: visiblePlugins.length };
    for (const a of installedAgents) counts[a.id] = 0;
    for (const p of visiblePlugins) {
      counts[p.agent] = (counts[p.agent] ?? 0) + 1;
    }
    return counts;
  }, [installedAgents, visiblePlugins]);

  useEffect(() => {
    if (!inspect.target) return;
    if (!plugins.some((p) => p.id === inspect.target?.id)) {
      inspect.close();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps -- close when the visible set drops the row
  }, [plugins, inspect.target?.id]);

  async function locateSource(path: string) {
    try {
      await openPathInFileManager(path);
    } catch (e) {
      toast({
        title: t('plugins.toast.cannotOpenDir'),
        description: e instanceof Error ? e.message : String(e),
        variant: 'danger',
      });
    }
  }

  const inspectPanel = inspect.target ? (
    <PluginDetailPanel
      plugin={inspect.target}
      width={inspect.paneWidth}
      onClose={() => inspect.close()}
      onLocate={locateSource}
    />
  ) : null;

  return (
    <WorkbenchSplitPage
      split={inspect}
      resizeAria={t('common.resizeSidePanel')}
      panel={inspectPanel}
      header={(
        <PageHeader
          size="compact"
          title={t('plugins.page.title')}
          description={
            data
              ? t('plugins.page.descriptionCount', { n: visiblePlugins.length })
              : t('plugins.page.description')
          }
          descriptionTip={t('plugins.page.descriptionTip')}
          actions={
            <Button
              size="sm"
              variant="secondary"
              disabled={loading}
              onClick={() => void load()}
              className="gap-1.5"
            >
              {loading ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <RefreshCw className="h-3.5 w-3.5" />}
              {t('plugins.page.refresh')}
            </Button>
          }
        />
      )}
    >
      <div className={pageRhythm.chrome}>
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
              ? t('plugins.page.countAll', { n })
              : t('plugins.page.countAgent', { name: agentName(id), n })
          }
          emptyLabel={t('plugins.page.emptyTabs')}
          aria-label={t('plugins.page.filterAria')}
        />
      </div>
      {loading && !data ? (
        <ListSkeleton rows={4} />
      ) : error && !data ? (
        <ErrorState error={error} onRetry={() => void load()} />
      ) : plugins.length === 0 ? (
        <EmptyState
          icon={Puzzle}
          title={t('plugins.empty.title')}
          description={
            filterAgent === 'all'
              ? t('plugins.empty.all')
              : t('plugins.empty.agent', { name: agentName(filterAgent) })
          }
          action={
            <Button size="sm" variant="outline" className="mt-2" onClick={() => void load()}>
              {t('plugins.empty.refresh')}
            </Button>
          }
        />
      ) : (
        <PluginPackList
          plugins={plugins}
          showAgent={filterAgent === 'all'}
          activeId={inspect.target?.id ?? null}
          onOpen={(plugin) => inspect.open(plugin)}
        />
      )}
    </WorkbenchSplitPage>
  );
}
