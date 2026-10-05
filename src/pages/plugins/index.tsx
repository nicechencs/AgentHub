import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Plus, Puzzle } from 'lucide-react';
import { AgentTabStrip, type AgentTabId } from '@/components/layout/AgentTabStrip';
import { PageHeader } from '@/components/layout/PageHeader';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { WorkbenchSplitPage } from '@/components/layout/SideSplit';
import { useSideSplit } from '@/components/layout/use-side-split';
import { EmptyState } from '@/components/shared/EmptyState';
import { ErrorState } from '@/components/shared/ErrorState';
import { Notice } from '@/components/shared/Notice';
import { useI18n } from '@/components/shared/LanguageProvider';
import { PageRefreshButton } from '@/components/shared/PageRefreshButton';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { ListSkeleton } from '@/components/ui/skeleton';
import { useToast } from '@/components/ui/toast';
import { agentDisplayName } from '@/config/agents';
import { filterByPageVisibleAgent } from '@/lib/agent-visibility';
import { useInstalledAgents } from '@/lib/hooks/useInstalledAgents';
import {
  disablePlugin,
  enablePlugin,
  installPlugin,
  listPluginInventory,
  refreshPluginMarketplace,
  uninstallPlugin,
  updatePiPlugins,
  updatePlugin,
} from '@/lib/api/plugins';
import { openPathInFileManager } from '@/lib/api/skill';
import type {
  PluginEntry,
  PluginInventory,
  PluginMutationOutcome,
} from '@/lib/backend/contracts/plugin-types';
import type { AgentKey } from '@/lib/types';
import { canInstallListedPlugin } from './can-install';
import { canRefreshPluginMarketplace, canUpdateAllPlugins } from './can-update';
import { createExclusiveActionGate, createLatestRequestGate } from './latest-request';
import { PluginDetailPanel } from './PluginDetailPanel';
import { PluginInstallDialog } from './PluginInstallDialog';
import { PluginPackList } from './PluginPackList';
import { PluginUninstallDialog } from './PluginUninstallDialog';
import { PluginUpdateDialog, type PluginUpdateTarget } from './PluginUpdateDialog';
import { pluginEmptyCopy, pluginScanFailedAgents } from './plugin-empty';
import { StorageKey } from '@/lib/ui-preferences';

const PLUGINS_PREVIEW_WIDTH_KEY = StorageKey.pluginsPreviewWidth;
type PluginMutation = 'install' | 'uninstall' | 'toggle' | 'marketplace' | 'update';

function agentName(id: AgentKey): string {
  return agentDisplayName(id);
}

export default function PluginsPage() {
  const { t, lang } = useI18n();
  const { toast } = useToast();
  const { hiddenIds, installedIds, installedAgents, loading: agentsLoading } = useInstalledAgents();
  const [data, setData] = useState<PluginInventory | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<Error | string | null>(null);
  const [filterAgent, setFilterAgent] = useState<AgentTabId>('all');
  const [installOpen, setInstallOpen] = useState(false);
  const [installError, setInstallError] = useState<unknown>(null);
  const [uninstallTarget, setUninstallTarget] = useState<PluginEntry | null>(null);
  const [uninstallError, setUninstallError] = useState<unknown>(null);
  const [updateTarget, setUpdateTarget] = useState<PluginUpdateTarget | null>(null);
  const [updateError, setUpdateError] = useState<unknown>(null);
  const [busyAction, setBusyAction] = useState<PluginMutation | null>(null);
  const mutationGate = useRef(createExclusiveActionGate());
  const loadGate = useRef(createLatestRequestGate());
  const inspect = useSideSplit<PluginEntry>({ storageKey: PLUGINS_PREVIEW_WIDTH_KEY });
  const mutationBusy = busyAction !== null;
  const installBusy = busyAction === 'install';
  const uninstallBusy = busyAction === 'uninstall';
  const marketBusy = busyAction === 'marketplace';
  const updateBusy = busyAction === 'update';
  const showInstall = filterAgent === 'all' || canInstallListedPlugin(filterAgent);
  const showMarketplaceRefresh =
    filterAgent !== 'all' && canRefreshPluginMarketplace(filterAgent);
  const showUpdateAll = filterAgent !== 'all' && canUpdateAllPlugins(filterAgent);

  const load = useCallback(async (): Promise<PluginInventory | null> => {
    const generation = loadGate.current.begin();
    setLoading(true);
    setError(null);
    try {
      const inv = await listPluginInventory();
      if (!loadGate.current.isCurrent(generation)) return null;
      setData(inv);
      return inv;
    } catch (e) {
      if (!loadGate.current.isCurrent(generation)) return null;
      setError(e instanceof Error ? e : String(e));
      setData(null);
      return null;
    } finally {
      if (loadGate.current.isCurrent(generation)) setLoading(false);
    }
  }, []);

  function applyMutationOutcome(outcome: PluginMutationOutcome): boolean {
    // The desktop command produced this scan while still holding its write
    // lock. Do not replace it with a later unlocked scan before deciding
    // whether the mutation is safe to present as successful.
    setData(outcome.inventory);
    setError(null);
    setLoading(false);
    return outcome.status === 'confirmed';
  }

  useEffect(() => {
    void load();
    return () => loadGate.current.invalidate();
  }, [load]);

  const beginMutation = useCallback((action: PluginMutation): boolean => {
    if (!mutationGate.current.begin()) return false;
    setBusyAction(action);
    return true;
  }, []);

  const endMutation = useCallback((action: PluginMutation) => {
    mutationGate.current.end();
    setBusyAction((current) => (current === action ? null : current));
  }, []);

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

  const failedAgents = pluginScanFailedAgents(
    data?.agents,
    new Set(installedIds),
  );
  const failedNames = failedAgents
    .map((row) => agentName(row.agent))
    .join(lang === 'en' ? ', ' : '、');
  const emptyCopy = pluginEmptyCopy(
    filterAgent,
    data?.agents,
    filterAgent === 'all' ? '' : agentName(filterAgent),
    t,
    filterAgent === 'all' ? failedNames : '',
  );

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

  async function runInstall(agent: AgentKey, source: string, confirmed: boolean) {
    if (!beginMutation('install')) return;
    setInstallError(null);
    try {
      const outcome = await installPlugin(agent, source, { confirmed });
      if (!applyMutationOutcome(outcome)) {
        setInstallError(t('plugins.outcome.unconfirmed'));
        return;
      }
      setInstallOpen(false);
      toast({ title: t('plugins.install.ok'), variant: 'success' });
    } catch (e) {
      setInstallError(e instanceof Error ? e : String(e));
    } finally {
      endMutation('install');
    }
  }

  async function runUninstall(plugin: PluginEntry, keepData: boolean) {
    if (!beginMutation('uninstall')) return;
    setUninstallError(null);
    try {
      const outcome = await uninstallPlugin(
        plugin.agent,
        plugin.name,
        plugin.marketplace,
        plugin.installSource,
        { keepData },
      );
      if (!applyMutationOutcome(outcome)) {
        setUninstallError(t('plugins.outcome.unconfirmed'));
        return;
      }
      inspect.close();
      setUninstallTarget(null);
      toast({ title: t('plugins.uninstall.ok'), variant: 'success' });
    } catch (e) {
      setUninstallError(e instanceof Error ? e : String(e));
    } finally {
      endMutation('uninstall');
    }
  }

  async function togglePlugin(plugin: PluginEntry, enabled: boolean) {
    if (!beginMutation('toggle')) return;
    try {
      const outcome = enabled
        ? await enablePlugin(plugin.agent, plugin.name, plugin.marketplace)
        : await disablePlugin(plugin.agent, plugin.name, plugin.marketplace);
      const confirmed = applyMutationOutcome(outcome);
      const next = outcome.inventory.plugins.find((row) => row.id === plugin.id);
      if (next) inspect.open(next);
      if (!confirmed) {
        toast({
          title: t('plugins.outcome.unconfirmed'),
          variant: 'danger',
        });
        return;
      }
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
    } finally {
      endMutation('toggle');
    }
  }

  async function refreshMarketplace() {
    if (filterAgent === 'all' || !canRefreshPluginMarketplace(filterAgent)) return;
    if (!beginMutation('marketplace')) return;
    try {
      const outcome = await refreshPluginMarketplace(filterAgent);
      if (!applyMutationOutcome(outcome)) {
        toast({ title: t('plugins.outcome.unconfirmed'), variant: 'danger' });
        return;
      }
      toast({ title: t('plugins.marketplace.ok'), variant: 'success' });
    } catch (e) {
      toast({
        title: t('plugins.marketplace.failed'),
        description: e instanceof Error ? e.message : String(e),
        variant: 'danger',
      });
    } finally {
      endMutation('marketplace');
    }
  }

  async function runUpdate(target: PluginUpdateTarget) {
    if (!beginMutation('update')) return;
    setUpdateError(null);
    try {
      const outcome = target.kind === 'pi-all'
        ? await updatePiPlugins({ confirmed: true })
        : await updatePlugin(
            target.plugin.agent,
            target.plugin.name,
            target.plugin.marketplace,
            target.plugin.scope,
            { confirmed: true },
          );
      const confirmed = applyMutationOutcome(outcome);
      const inspectedId = inspect.target?.id ?? null;
      const next = outcome.inventory.plugins.find((row) => row.id === inspectedId);
      if (next) inspect.open(next);
      if (!confirmed) {
        setUpdateError(t('plugins.outcome.unconfirmed'));
        return;
      }
      setUpdateTarget(null);
      toast({ title: t('plugins.update.ok'), variant: 'success' });
    } catch (e) {
      setUpdateError(e instanceof Error ? e : String(e));
    } finally {
      endMutation('update');
    }
  }

  const inspectPanel = inspect.target ? (
    <PluginDetailPanel
      plugin={inspect.target}
      width={inspect.paneWidth}
      onClose={() => inspect.close()}
      onLocate={locateSource}
      onToggle={togglePlugin}
      disabled={mutationBusy || loading}
      onUpdate={(plugin) => {
        setUpdateError(null);
        setUpdateTarget({ kind: 'plugin', plugin });
      }}
      onUninstall={(plugin) => {
        setUninstallError(null);
        setUninstallTarget(plugin);
      }}
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
              ? t('plugins.page.countAll', { n })
              : t('plugins.page.countAgent', { name: agentName(id), n })
          }
          emptyLabel={t('plugins.page.emptyTabs')}
          aria-label={t('plugins.page.filterAria')}
        />
        <div className={pageRhythm.chromeActions}>
          {showMarketplaceRefresh ? (
            <Button
              size="sm"
              variant="outline"
              disabled={mutationBusy || loading}
              onClick={() => void refreshMarketplace()}
            >
              {marketBusy ? t('plugins.marketplace.refreshing') : t('plugins.marketplace.button')}
            </Button>
          ) : null}
          {showUpdateAll ? (
            <Button
              size="sm"
              variant="outline"
              disabled={mutationBusy || loading}
              onClick={() => {
                setUpdateError(null);
                setUpdateTarget({ kind: 'pi-all' });
              }}
            >
              {t('plugins.update.piButton')}
            </Button>
          ) : null}
          {showInstall ? (
            <Button
              size="sm"
              disabled={mutationBusy || loading}
              onClick={() => {
                setInstallError(null);
                setInstallOpen(true);
              }}
            >
              <Plus className="h-3.5 w-3.5" /> {t('plugins.install.button')}
            </Button>
          ) : null}
          <PageRefreshButton
            loading={loading}
            disabled={mutationBusy}
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
          title={emptyCopy.title}
          description={emptyCopy.description}
          action={
            showInstall ? (
              <Button
                size="sm"
                className="mt-2"
                disabled={mutationBusy || loading}
                onClick={() => {
                  setInstallError(null);
                  setInstallOpen(true);
                }}
              >
                <Plus className="h-3.5 w-3.5" /> {t('plugins.install.button')}
              </Button>
            ) : emptyCopy.showRefresh ? (
              <Button
                size="sm"
                variant="outline"
                className="mt-2"
                disabled={mutationBusy || loading}
                onClick={() => void load()}
              >
                {t('plugins.empty.refresh')}
              </Button>
            ) : undefined
          }
        />
      ) : (
        <>
          {filterAgent === 'all' && failedNames ? (
            <Notice tone="warning" className="mb-3">
              {t('plugins.empty.scanFailed', { names: failedNames })}
            </Notice>
          ) : null}
          <PluginPackList
            plugins={plugins}
            showAgent={filterAgent === 'all'}
            activeId={inspect.target?.id ?? null}
            onOpen={(plugin) => inspect.open(plugin)}
          />
        </>
      )}
      <PluginInstallDialog
        open={installOpen}
        defaultAgent={filterAgent === 'all' ? 'grok' : filterAgent}
        busy={installBusy}
        error={installError}
        onClose={() => {
          if (installBusy) return;
          setInstallOpen(false);
          setInstallError(null);
        }}
        onInstall={runInstall}
      />
      <PluginUninstallDialog
        plugin={uninstallTarget}
        busy={uninstallBusy}
        error={uninstallError}
        onClose={() => {
          if (uninstallBusy) return;
          setUninstallTarget(null);
          setUninstallError(null);
        }}
        onUninstall={runUninstall}
      />
      <PluginUpdateDialog
        target={updateTarget}
        busy={updateBusy}
        error={updateError}
        onClose={() => {
          if (updateBusy) return;
          setUpdateTarget(null);
          setUpdateError(null);
        }}
        onConfirm={runUpdate}
      />
    </WorkbenchSplitPage>
  );
}
