import * as React from 'react';
import { PackageSearch } from 'lucide-react';
import { PageHeader } from '@/components/layout/PageHeader';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { EmptyState } from '@/components/shared/EmptyState';
import { EnvRemediationPanel } from '@/components/shared/EnvRemediationPanel';
import { EnvStatusBar } from '@/components/shared/EnvStatusBar';
import { ErrorState } from '@/components/shared/ErrorState';
import { ListSkeleton } from '@/components/ui/skeleton';
import { Tip } from '@/components/ui/tooltip';
import {
  applyAgentUpdates,
  checkAgentUpdates,
  listAgents,
} from '@/lib/api/agent';
import { tryRefreshDoctor } from '@/lib/api/doctor';
import { listRuntimes, resolveAutoInstallPlan } from '@/lib/api/env';
import { hasEnvIssues } from '@/lib/env';
import type { AgentId, AgentStatus, RuntimeDetect, RuntimeId } from '@/lib/types';
import { AgentCard } from './agent-card';

/** Agents 安装管理页 — 环境检测 + Agent 安装（backend 由构建时 composition root 选择） */
export default function AgentsPage() {
  const [agents, setAgents] = React.useState<AgentStatus[]>([]);
  const [runtimes, setRuntimes] = React.useState<RuntimeDetect[]>([]);
  const [loading, setLoading] = React.useState(true);
  const [envLoading, setEnvLoading] = React.useState(false);
  const [error, setError] = React.useState<unknown>(null);
  /** 页级修复面板:focus + 是否自动开装 */
  const [pageFix, setPageFix] = React.useState<{
    runtimeId?: RuntimeId;
    autoStart: boolean;
  } | null>(null);
  /** 真实安装中态(勿用 autoStart 充当 busy,失败后会永久卡住) */
  const [envInstallRunning, setEnvInstallRunning] = React.useState(false);
  const updateSeq = React.useRef(0);

  const mergeUpdates = React.useCallback(async (list: AgentStatus[], force = false) => {
    const seq = ++updateSeq.current;
    const installedIds = list.filter((a) => a.installed).map((a) => a.agentId);
    if (!installedIds.length) return;

    // Mark checking so cards can show loading on upgrade button
    setAgents((prev) =>
      prev.map((a) =>
        installedIds.includes(a.agentId)
          ? {
              ...a,
              update: {
                agentId: a.agentId,
                state: 'checking',
                currentVersion: a.version,
                latestVersion: a.latestVersion,
              },
            }
          : a,
      ),
    );

    try {
      const updates = await checkAgentUpdates(installedIds, force);
      if (seq !== updateSeq.current) return;
      setAgents((prev) => applyAgentUpdates(prev, updates));
    } catch {
      if (seq !== updateSeq.current) return;
      // Fail closed: unknown, never pretend up-to-date
      setAgents((prev) =>
        prev.map((a) =>
          installedIds.includes(a.agentId)
            ? {
                ...a,
                update: {
                  agentId: a.agentId,
                  state: 'unknown',
                  currentVersion: a.version,
                  note: '更新检查失败，仍可强制升级',
                },
              }
            : a,
        ),
      );
    }
  }, []);

  const load = React.useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [a, r] = await Promise.all([listAgents(), listRuntimes()]);
      setAgents(a);
      setRuntimes(r);
      void mergeUpdates(a, false);
    } catch (e) {
      setError(e);
    } finally {
      setLoading(false);
    }
  }, [mergeUpdates]);

  React.useEffect(() => {
    void load();
  }, [load]);

  const refreshAgents = React.useCallback(() => {
    listAgents({ force: true })
      .then((a) => {
        setAgents(a);
        void mergeUpdates(a, true);
      })
      .catch(() => {});
  }, [mergeUpdates]);

  const refreshAgentUpdate = React.useCallback(
    (agentId: AgentId) => {
      void (async () => {
        try {
          const updates = await checkAgentUpdates([agentId], true);
          setAgents((prev) => applyAgentUpdates(prev, updates));
        } catch {
          /* keep previous update state */
        }
      })();
    },
    [],
  );

  const refreshEnv = React.useCallback(async () => {
    setEnvLoading(true);
    try {
      // 「重新检测」绕过 doctor TTL + 后端 detect 缓存
      const forced = await tryRefreshDoctor();
      const r = forced?.runtimes ?? (await listRuntimes());
      setRuntimes(r);
      const nextAgents = forced?.agents ?? (await listAgents({ force: true }));
      setAgents(nextAgents);
      void mergeUpdates(nextAgents, true);
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
  }, [mergeUpdates]);

  const pageFixRuntime = pageFix?.runtimeId
    ? runtimes.find((r) => r.id === pageFix.runtimeId)
    : runtimes.find((r) => r.status !== 'ok');

  const showPagePanel = pageFix != null && hasEnvIssues(runtimes);

  return (
    <div>
      <PageHeader
        title="Agent 管理"
        description="环境修复与安装"
        descriptionTip="检测并修复共享运行时（如 Node），再安装或升级 Claude / Codex / Kimi / Grok / Pi / WorkBuddy / Cursor Agent 等 CLI。"
      />

      <div className={pageRhythm.lead}>
        <EnvStatusBar
          runtimes={runtimes}
          loading={loading || envLoading}
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
        {!loading && hasEnvIssues(runtimes) && !showPagePanel && (
          <Tip
            className="text-xs text-muted"
            label="「一键修复」仅安装可自动处理的 runtime（如 Node）。卸载 Agent 不会卸载共享环境。若 PATH 仍异常，请完全退出并重启 AgentHub 后再检测。"
          >
            环境未就绪，可先一键修复
          </Tip>
        )}
      </div>

      {loading ? (
        <ListSkeleton rows={4} />
      ) : error ? (
        <ErrorState error={error} onRetry={() => void load()} />
      ) : agents.length === 0 ? (
        <EmptyState
          icon={PackageSearch}
          title="未检测到 Agent"
          description="确认 CLI 已安装后重试"
          actionLabel="重新检测"
          onAction={() => void load()}
        />
      ) : (
        <div className={pageRhythm.stack}>
          {agents.map((a) => (
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
