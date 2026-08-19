import * as React from 'react';
import { PackageSearch } from 'lucide-react';
import { getAgentStatusSnapshot, useAgentStatuses } from '@/app/runtime';
import { PageHeader } from '@/components/layout/PageHeader';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { EmptyState } from '@/components/shared/EmptyState';
import { useI18n } from '@/components/shared/LanguageProvider';
import { EnvRemediationPanel } from '@/components/shared/EnvRemediationPanel';
import { EnvStatusBar } from '@/components/shared/EnvStatusBar';
import { ErrorState } from '@/components/shared/ErrorState';
import { ListSkeleton } from '@/components/ui/skeleton';
import { Tip } from '@/components/ui/tooltip';
import { sortAgentsForManagePage } from '@/lib/agent-visibility';
import { applyAgentUpdates, checkAgentUpdates } from '@/lib/api/agent';
import { tryRefreshDoctor } from '@/lib/api/doctor';
import { listRuntimes, resolveAutoInstallPlan } from '@/lib/api/env';
import { hasEnvIssues } from '@/lib/env';
import type { AgentId, AgentStatus, AgentUpdateInfo, RuntimeDetect, RuntimeId } from '@/lib/types';
import { AgentCard } from './agent-card';

/** Agents 安装管理页 — 环境检测 + Agent 安装（backend 由构建时 composition root 选择） */
export default function AgentsPage() {
  const { t } = useI18n();
  const { state, statuses, error, reload } = useAgentStatuses();
  const [updateById, setUpdateById] = React.useState<
    Partial<Record<AgentId, AgentUpdateInfo>>
  >({});
  const [runtimes, setRuntimes] = React.useState<RuntimeDetect[]>([]);
  const [envLoading, setEnvLoading] = React.useState(true);
  const [envError, setEnvError] = React.useState<unknown>(null);
  /** 页级修复面板:focus + 是否自动开装 */
  const [pageFix, setPageFix] = React.useState<{
    runtimeId?: RuntimeId;
    autoStart: boolean;
  } | null>(null);
  /** 真实安装中态(勿用 autoStart 充当 busy,失败后会永久卡住) */
  const [envInstallRunning, setEnvInstallRunning] = React.useState(false);
  const updateSeq = React.useRef(0);
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

  const loadRuntimes = React.useCallback(async () => {
    setEnvLoading(true);
    setEnvError(null);
    try {
      setRuntimes(await listRuntimes());
    } catch (e) {
      setEnvError(e);
    } finally {
      setEnvLoading(false);
    }
  }, []);

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

  const refreshAgentUpdate = React.useCallback((agentId: AgentId) => {
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
  }, [mergeUpdates, reload]);

  const pageFixRuntime = pageFix?.runtimeId
    ? runtimes.find((r) => r.id === pageFix.runtimeId)
    : runtimes.find((r) => r.status !== 'ok');

  const showPagePanel = pageFix != null && hasEnvIssues(runtimes);
  const orderedAgents = React.useMemo(() => sortAgentsForManagePage(agents), [agents]);
  const showAgentSkeleton = statuses.length === 0 && (state === 'idle' || state === 'loading');
  const pageError =
    statuses.length === 0 ? (state === 'error' ? error : envError) : null;

  return (
    <div>
      <PageHeader
        title={t('agents.page.title')}
        description={t('agents.page.description')}
        descriptionTip={t('agents.page.descriptionTip')}
      />

      <div className={pageRhythm.lead}>
        <EnvStatusBar
          runtimes={runtimes}
          loading={showAgentSkeleton || envLoading}
          onRefresh={() => void refreshEnv()}
          onFix={(r) => setPageFix({ runtimeId: r.id, autoStart: false })}
          onOneClickFix={() => setPageFix({ autoStart: true })}
          oneClickBusy={envInstallRunning}
        />
        {showPagePanel && (
          <EnvRemediationPanel
            key={`page-fix-${pageFix.runtimeId ?? 'all'}-${pageFix.autoStart}`}
            runtime={pageFixRuntime}
            runtimes={runtimes}
            focusIds={pageFix.runtimeId ? [pageFix.runtimeId] : undefined}
            autoStart={pageFix.autoStart}
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
        <div className={pageRhythm.stack}>
          {orderedAgents.map((a) => (
            <AgentCard
              key={a.agentId}
              agent={a}
              runtimes={runtimes}
              onChanged={refreshAgents}
              onEnvChanged={() => void refreshEnv()}
              onRecheckUpdate={() => refreshAgentUpdate(a.agentId)}
            />
          ))}
        </div>
      )}
    </div>
  );
}
