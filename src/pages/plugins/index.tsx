import { useCallback, useEffect, useMemo, useState } from 'react';
import { Puzzle } from 'lucide-react';
import { AgentTabStrip, type AgentTabId } from '@/components/layout/AgentTabStrip';
import { PageHeader } from '@/components/layout/PageHeader';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { WorkbenchSplitPage } from '@/components/layout/SideSplit';
import { useSideSplit } from '@/components/layout/use-side-split';
import { EmptyState } from '@/components/shared/EmptyState';
import { ErrorState } from '@/components/shared/ErrorState';
import { useI18n } from '@/components/shared/LanguageProvider';
import { PageRefreshButton } from '@/components/shared/PageRefreshButton';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { ListSkeleton } from '@/components/ui/skeleton';
import { useToast } from '@/components/ui/toast';
import { agentDisplayName } from '@/config/agents';
import { filterByPageVisibleAgent } from '@/lib/agent-visibility';
import { useInstalledAgents } from '@/lib/hooks/useInstalledAgents';
import { disablePlugin, enablePlugin, listPluginInventory } from '@/lib/api/plugins';
import { openPathInFileManager } from '@/lib/api/skill';
import type { PluginEntry, PluginInventory } from '@/lib/backend/contracts/plugin-types';
import type { AgentKey } from '@/lib/types';
import { PluginDetailPanel } from './PluginDetailPanel';
import { PluginPackList } from './PluginPackList';
import { StorageKey } from '@/lib/ui-preferences';

const PLUGINS_PREVIEW_WIDTH_KEY = StorageKey.pluginsPreviewWidth;

function agentName(id: AgentKey): string {
  return agentDisplayName(id);
}

export default function PluginsPage() {
  const { t } = useI18n();
  const { toast } = useToast();
  const { hiddenIds, installedIds, installedAgents, loading: agentsLoading } = useInstalledAgents();
  const [data, setData] = useState<PluginInventory | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<Error | string | null>(null);
  const [filterAgent, setFilterAgent] = useState<AgentTabId>('all');
  const inspect = useSideSplit<PluginEntry>({ storageKey: PLUGINS_PREVIEW_WIDTH_KEY });

  const load = useCallback(async (): Promise<PluginInventory | null> => {
    setLoading(true);
    setError(null);
    try {
      const inv = await listPluginInventory();
      setData(inv);
      return inv;
    } catch (e) {
      setError(e instanceof Error ? e : String(e));
      setData(null);
      return null;
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
    return filterByPageVisibleAgent(
      data.plugins,
      (p) => p.agent,
      hiddenIds,
      installedIds,
      !agentsLoading,
    );
  }, [data, hiddenIds, installedIds, agentsLoading]);

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

  async function togglePlugin(plugin: PluginEntry, enabled: boolean) {
    try {
      if (enabled) {
        await enablePlugin(plugin.agent, plugin.name, plugin.marketplace);
      } else {
        await disablePlugin(plugin.agent, plugin.name, plugin.marketplace);
      }
      const inv = await load();
      const next = inv?.plugins.find((row) => row.id === plugin.id);
      if (next) inspect.open(next);
      toast({
        title: enabled ? t('plugins.actions.enabled') : t('plugins.actions.disabled'),
        variant: 'success',
      });
    } catch (e) {
      toast({
        title: enabled ? t('plugins.actions.enableFailed') : t('plugins.actions.disableFailed'),
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
      onToggle={togglePlugin}
    />
  ) : null;

  return (
    <WorkbenchSplitPage
      split={inspect}
      resizeAria={t('common.resizeSidePanel')}
      panel={inspectPanel}
    >
      <PageHeader
        title={t('plugins.page.title')}
        badge={<Badge variant="default">{t('common.inDevelopment')}</Badge>}
        description={
          data
            ? t('plugins.page.descriptionCount', { n: visiblePlugins.length })
            : t('plugins.page.description')
        }
        descriptionTip={t('plugins.page.descriptionTip')}
      />
      <div className={pageRhythm.chromeRow}>
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
        <div className={pageRhythm.chromeActions}>
          <PageRefreshButton
            loading={loading}
            onClick={() => void load()}
            label={t('plugins.page.refresh')}
          />
        </div>
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
