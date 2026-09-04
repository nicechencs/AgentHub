import * as React from 'react';
import { PackageSearch } from 'lucide-react';
import { getAgentStatusSnapshot, useAgentStatuses } from '@/app/runtime';
import { PageHeader } from '@/components/layout/PageHeader';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { WorkbenchSplitPage } from '@/components/layout/SideSplit';
import { useSideSplit } from '@/components/layout/use-side-split';
import { EmptyState } from '@/components/shared/EmptyState';
import { EnvRemediationPanel } from '@/components/shared/EnvRemediationPanel';
import { ErrorState } from '@/components/shared/ErrorState';
import { useI18n } from '@/components/shared/LanguageProvider';
import { ListSkeleton } from '@/components/ui/skeleton';
import {
  ColumnResizeHandle,
  Table,
  TableBody,
  TableHead,
  TableHeader,
  TableHeaderRow,
  TableShell,
  useColumnWidths,
} from '@/components/ui/table';
import { Tip } from '@/components/ui/tooltip';
import { SortHandle } from '@/components/shared/SortHandle';
import { SORTABLE_ID_ATTR, useSortableDrag } from '@/components/shared/use-sortable-drag';
import { useStoredIdOrder } from '@/components/shared/use-stored-id-order';
import { resolveAgentMeta } from '@/config/agents';
import { applyStoredAgentOrder, sortAgentsForManagePage } from '@/lib/agent-visibility';
import { applyAgentUpdates, checkAgentUpdates } from '@/lib/api/agent';
import { StorageKey } from '@/lib/ui-preferences';
import { tryRefreshDoctor } from '@/lib/api/doctor';
import { checkRuntimeUpdates, listRuntimes, resolveAutoInstallPlan } from '@/lib/api/env';
import { hasEnvIssues } from '@/lib/env';
import type { AgentKey, AgentStatus, AgentUpdateInfo, RuntimeDetect, RuntimeId, RuntimeUpdateInfo } from '@/lib/types';
import { AgentCard } from './agent-card';
import { AgentDetailPanel } from './AgentDetailPanel';
import { EnvSoftwareList, type EnvSoftwareIntent } from './EnvSoftwareList';
import { cn } from '@/lib/utils';
import {
  AGENT_TABLE_COLUMN_SPECS,
  AGENT_TABLE_FIXED_COLUMN_SPECS,
  AGENT_TABLE_FLEX_COLUMN,
  agentTableColumnLabel,
  agentTableColumnSide,
} from './agent-table';

const AGENTS_PREVIEW_WIDTH_KEY = StorageKey.agentsPreviewWidth;

/** Agents 安装管理页 — 环境检测 + Agent 安装（backend 由构建时 composition root 选择） */
export default function AgentsPage() {
  const { t } = useI18n();
  const { state, statuses, error, reload } = useAgentStatuses();
  const [updateById, setUpdateById] = React.useState<
    Partial<Record<AgentKey, AgentUpdateInfo>>
  >({});
  const [runtimes, setRuntimes] = React.useState<RuntimeDetect[]>([]);
  const [envLoading, setEnvLoading] = React.useState(true);
  const [envError, setEnvError] = React.useState<unknown>(null);
  const [runtimeUpdates, setRuntimeUpdates] = React.useState<
    Partial<Record<RuntimeId, RuntimeUpdateInfo>>
  >({});
  /** 页级修复面板:focus + 是否自动开装 */
  const [pageFix, setPageFix] = React.useState<{
    runtimeId?: RuntimeId;
    autoStart: boolean;
    intent: EnvSoftwareIntent;
    canAutoUpgrade?: boolean;
  } | null>(null);
  /** 真实安装中态(勿用 autoStart 充当 busy,失败后会永久卡住) */
  const [envInstallRunning, setEnvInstallRunning] = React.useState(false);
  const updateSeq = React.useRef(0);
  const runtimeUpdateSeq = React.useRef(0);
  const initialUpdatesStarted = React.useRef(false);

  const agents = React.useMemo(() => {
    const updates = Object.values(updateById).filter(
      (row): row is AgentUpdateInfo => row != null,
    );
    return applyAgentUpdates(statuses, updates);
  }, [statuses, updateById]);

  const mergeUpdates = React.useCallback(async (list: AgentStatus[], force = false) => {
    const seq = ++updateSeq.current;
    const installed = list.filter((a) => a.installed);
    if (!installed.length) return;

    // Mark checking so cards can show loading on upgrade button
    setUpdateById((prev) => {
      const next = { ...prev };
      for (const agent of installed) {
        next[agent.agentId] = {
          agentId: agent.agentId,
          state: 'checking',
          currentVersion: agent.version,
          latestVersion: agent.latestVersion,
        };
      }
      return next;
    });

    try {
      const updates = await checkAgentUpdates(
        installed.map((a) => a.agentId),
        force,
      );
      if (seq !== updateSeq.current) return;
      setUpdateById((prev) => {
        const next = { ...prev };
        for (const update of updates) next[update.agentId] = update;
        return next;
      });
    } catch {
      if (seq !== updateSeq.current) return;
      // Fail closed: unknown, never pretend up-to-date
      setUpdateById((prev) => {
        const next = { ...prev };
        for (const agent of installed) {
          next[agent.agentId] = {
            agentId: agent.agentId,
            state: 'unknown',
            currentVersion: agent.version,
            note: t('agents.page.updateCheckFailed'),
          };
        }
        return next;
      });
    }
  }, [t]);

  const loadRuntimeUpdates = React.useCallback(async (list: RuntimeDetect[], force = false) => {
    const seq = ++runtimeUpdateSeq.current;
    try {
      const updates = await checkRuntimeUpdates(list.map((runtime) => runtime.id), force);
      if (seq !== runtimeUpdateSeq.current) return;
      setRuntimeUpdates(Object.fromEntries(updates.map((update) => [update.runtimeId, update])));
    } catch {
      // Keep the previous result if the desktop command itself is unavailable.
      // Core turns ordinary network errors into an explicit unknown state.
    }
  }, []);

  const loadRuntimes = React.useCallback(async () => {
    setEnvLoading(true);
    setEnvError(null);
    try {
      const next = await listRuntimes();
      setRuntimes(next);
      void loadRuntimeUpdates(next);
    } catch (e) {
      setEnvError(e);
    } finally {
      setEnvLoading(false);
    }
  }, [loadRuntimeUpdates]);

  React.useEffect(() => {
    void loadRuntimes();
  }, [loadRuntimes]);

  React.useEffect(() => {
    if (initialUpdatesStarted.current) return;
    if (state !== 'ready') return;
    initialUpdatesStarted.current = true;
    if (statuses.some((row) => row.installed)) {
      void mergeUpdates(statuses, false);
    }
  }, [state, statuses, mergeUpdates]);

  const refreshAgents = React.useCallback(() => {
    void (async () => {
      try {
        await reload();
        void mergeUpdates(getAgentStatusSnapshot().statuses, true);
      } catch {
        /* store keeps the last good snapshot */
      }
    })();
  }, [reload, mergeUpdates]);

  const refreshAgentUpdate = React.useCallback((agentId: AgentKey) => {
    void (async () => {
      try {
        const updates = await checkAgentUpdates([agentId], true);
        setUpdateById((prev) => {
          const next = { ...prev };
          for (const update of updates) next[update.agentId] = update;
          return next;
        });
      } catch {
        /* keep previous update state */
      }
    })();
  }, []);

  const retry = React.useCallback(() => {
    initialUpdatesStarted.current = false;
    void reload().catch(() => {});
    void loadRuntimes();
  }, [reload, loadRuntimes]);

  const refreshEnv = React.useCallback(async () => {
    setEnvLoading(true);
    try {
      // 「重新检测」绕过 doctor TTL + 后端 detect 缓存
      const forced = await tryRefreshDoctor();
      const r = forced?.runtimes ?? (await listRuntimes());
      setRuntimes(r);
      void loadRuntimeUpdates(r, true);
      setEnvError(null);
      try {
        await reload();
      } catch {
        /* store keeps the last good snapshot */
      }
      void mergeUpdates(getAgentStatusSnapshot().statuses, true);
      setPageFix((prev) => {
        if (!prev) return null;
        if (prev.runtimeId) {
          const updated = r.find((x) => x.id === prev.runtimeId);
          return updated && updated.status !== 'ok' ? { ...prev, autoStart: false } : null;
        }
        // 全量一键:若仍有可修项则保留面板(不 auto)
        const plan = resolveAutoInstallPlan(r);
        return plan.targets.length || r.some((x) => x.status !== 'ok')
          ? { ...prev, autoStart: false }
          : null;
      });
    } finally {
      setEnvLoading(false);
    }
  }, [loadRuntimeUpdates, mergeUpdates, reload]);

  const pageFixRuntime = pageFix?.runtimeId
    ? runtimes.find((r) => r.id === pageFix.runtimeId)
    : runtimes.find((r) => r.status !== 'ok');

  const showPagePanel = pageFix != null;
  const agentOrder = useStoredIdOrder(StorageKey.agentsCatalogOrder);
  const orderedAgents = React.useMemo(() => {
    const baseline = sortAgentsForManagePage(agents);
    return applyStoredAgentOrder(baseline, (row) => row.agentId, agentOrder.stored);
  }, [agentOrder.stored, agents]);
  const liveIds = React.useMemo(
    () => orderedAgents.map((row) => row.agentId),
    [orderedAgents],
  );
  React.useEffect(() => {
    agentOrder.seedIfEmpty(liveIds);
  }, [agentOrder.seedIfEmpty, liveIds]);
  const canReorder = liveIds.length > 1;
  const { widths, onResizeStart, onResizeKeyDown, totalWidth } = useColumnWidths(
    AGENT_TABLE_FIXED_COLUMN_SPECS,
    StorageKey.agentsColumnWidths,
  );
  const { onDragStartId, rowProps } = useSortableDrag((fromId, toId) => {
    agentOrder.moveInLive(liveIds, fromId, toId);
  });
  const moveNeighbor = React.useCallback((id: string, direction: -1 | 1) => {
    const index = liveIds.indexOf(id);
    const next = liveIds[index + direction];
    if (!next) return;
    agentOrder.moveInLive(liveIds, id, next);
  }, [agentOrder.moveInLive, liveIds]);
  const showAgentSkeleton = statuses.length === 0 && (state === 'idle' || state === 'loading');
  const pageError =
    statuses.length === 0 ? (state === 'error' ? error : envError) : null;
  const inspect = useSideSplit<AgentKey>({ storageKey: AGENTS_PREVIEW_WIDTH_KEY });
  const inspectAgent = orderedAgents.find((row) => row.agentId === inspect.target);

  React.useEffect(() => {
    if (!inspect.target) return;
    if (!liveIds.includes(inspect.target)) inspect.close();
    // eslint-disable-next-line react-hooks/exhaustive-deps -- close when the selected agent leaves the list
  }, [inspect.target, liveIds]);

  const inspectPanel = inspectAgent ? (
    <AgentDetailPanel
      agent={inspectAgent}
      runtimes={runtimes}
      width={inspect.paneWidth}
      onClose={() => inspect.close()}
      onChanged={refreshAgents}
    />
  ) : null;

  return (
    <WorkbenchSplitPage
      split={inspect}
      resizeAria={t('common.resizeSidePanel')}
      panel={inspectPanel}
    >
      <PageHeader
        title={t('agents.page.title')}
        description={t('agents.page.description')}
        descriptionTip={t('agents.page.descriptionTip')}
      />
      <div className={pageRhythm.lead}>
        <EnvSoftwareList
          runtimes={runtimes}
          loading={showAgentSkeleton || envLoading}
          onRefresh={() => void refreshEnv()}
          onAction={(runtime, intent, canAutoUpgrade = true) =>
            setPageFix({
              runtimeId: runtime.id,
              autoStart: intent !== 'repair' && canAutoUpgrade,
              intent,
              canAutoUpgrade,
            })
          }
          onOneClickFix={() => setPageFix({ autoStart: true, intent: 'install' })}
          oneClickBusy={envInstallRunning}
          runtimeUpdates={runtimeUpdates}
        />
        {showPagePanel && (
          <EnvRemediationPanel
            key={`page-fix-${pageFix.runtimeId ?? 'all'}-${pageFix.autoStart}-${pageFix.intent}-${pageFix.canAutoUpgrade}`}
            runtime={pageFixRuntime}
            runtimes={runtimes}
            focusIds={pageFix.runtimeId ? [pageFix.runtimeId] : undefined}
            autoStart={pageFix.autoStart}
            intent={pageFix.intent}
            canAutoUpgrade={pageFix.canAutoUpgrade}
            pageHasPrimaryCta
            onRunningChange={setEnvInstallRunning}
            onDismiss={() => {
              setEnvInstallRunning(false);
              setPageFix(null);
            }}
            onDone={() => {
              setEnvInstallRunning(false);
              setPageFix(null);
              void refreshEnv();
            }}
          />
        )}
        {!showAgentSkeleton && hasEnvIssues(runtimes) && !showPagePanel && (
          <Tip
            className="text-xs text-muted"
            label={t('agents.page.envTip')}
          >
            {t('agents.page.envHint')}
          </Tip>
        )}
      </div>

      {showAgentSkeleton ? (
        <ListSkeleton rows={4} />
      ) : pageError ? (
        <ErrorState error={pageError} onRetry={retry} />
      ) : agents.length === 0 ? (
        <EmptyState
          icon={PackageSearch}
          title={t('agents.page.emptyTitle')}
          description={t('agents.page.emptyDesc')}
          actionLabel={t('agents.page.redetect')}
          onAction={retry}
        />
      ) : (
        <TableShell layout="split">
          <Table className="w-full table-fixed" style={{ minWidth: totalWidth }}>
            <colgroup>
              {AGENT_TABLE_COLUMN_SPECS.map((spec) => (
                <col
                  key={spec.key}
                  style={spec.key === AGENT_TABLE_FLEX_COLUMN ? undefined : { width: widths[spec.key] }}
                />
              ))}
            </colgroup>
            <TableHeader>
              <TableHeaderRow>
                {AGENT_TABLE_COLUMN_SPECS.map((spec) => {
                  const label = agentTableColumnLabel(spec.key, t);
                  const side = agentTableColumnSide(spec.key);
                  return (
                    <TableHead
                      key={spec.key}
                      className={cn('relative select-none', side === 'right' && 'text-right')}
                      data-col={spec.key}
                    >
                      {label}
                      {spec.key === AGENT_TABLE_FLEX_COLUMN ? null : (
                        <ColumnResizeHandle
                          columnKey={spec.key}
                          label={label}
                          onResizeStart={onResizeStart}
                          onResizeKeyDown={onResizeKeyDown}
                        />
                      )}
                    </TableHead>
                  );
                })}
              </TableHeaderRow>
            </TableHeader>
            <TableBody>
          {orderedAgents.map((a) => {
            const sortable = rowProps(a.agentId);
            return (
                <AgentCard
                  key={a.agentId}
                  agent={a}
                  runtimes={runtimes}
                  selected={inspect.target === a.agentId}
                  onSelect={() => inspect.open(a.agentId)}
                  onChanged={refreshAgents}
                  onEnvChanged={() => void refreshEnv()}
                  onRecheckUpdate={() => refreshAgentUpdate(a.agentId)}
                  sortId={sortable[SORTABLE_ID_ATTR]}
                  sortClassName={sortable.className}
                  sortHandle={canReorder ? (
                    <SortHandle
                      id={a.agentId}
                      color={resolveAgentMeta(a.agentId).color}
                      onDragStartId={onDragStartId}
                      onMoveNeighbor={moveNeighbor}
                    />
                  ) : null}
                />
            );
          })}
            </TableBody>
          </Table>
        </TableShell>
      )}
    </WorkbenchSplitPage>
  );
}
