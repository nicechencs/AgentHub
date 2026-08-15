import * as React from 'react';
import type { TerminalStatus } from '@/components/shared/InlineTerminal';
import { useToast } from '@/components/ui/toast';
import type { InstallChannelMeta } from '@/config/agents';
import {
  installAgentDetailed,
  InstallFailedError,
  uninstallAgentDetailed,
  upgradeAgentDetailed,
} from '@/lib/api/agent';
import { resolveAutoInstallPlan } from '@/lib/api/env';
import {
  isProgressForAgent,
  onInstallProgress,
} from '@/lib/api/install';
import { checkChannelEnv, formatMissingList } from '@/lib/env';
import type { AgentStatus, RuntimeDetect } from '@/lib/types';
import type { AgentCardConfirmKind } from './AgentCardDialogs';

export type AgentCardTask = {
  action: 'install' | 'upgrade' | 'oneclick';
  command: string;
  lines: string[];
  status: TerminalStatus;
};

const DONE_HOLD_MS = 500;

export function useAgentCardLifecycle(input: {
  agent: AgentStatus;
  agentName: string;
  runtimes: RuntimeDetect[];
  selectedChannel: InstallChannelMeta;
  selectedChannelId: string;
  setSelectedChannelId: (id: string) => void;
  onChanged: () => void;
  onEnvChanged: () => void;
  onRecheckUpdate?: () => void;
}) {
  const {
    agent,
    agentName,
    runtimes,
    selectedChannel,
    setSelectedChannelId,
    onChanged,
    onEnvChanged,
    onRecheckUpdate,
  } = input;
  const { toast } = useToast();
  const [task, setTask] = React.useState<AgentCardTask | null>(null);
  const cancelRef = React.useRef({ cancelled: false });
  const [elapsedSec, setElapsedSec] = React.useState(0);
  const progressUnsubRef = React.useRef<(() => void) | null>(null);
  const releaseProgressUnsub = React.useCallback(() => {
    const stop = progressUnsubRef.current;
    progressUnsubRef.current = null;
    if (typeof stop === 'function') stop();
  }, []);

  const [confirmDialog, setConfirmDialog] = React.useState<AgentCardConfirmKind>(null);
  const [confirmName, setConfirmName] = React.useState('');
  const [uninstalling, setUninstalling] = React.useState(false);
  const [showEnvPanel, setShowEnvPanel] = React.useState(false);
  const [envAutoStart, setEnvAutoStart] = React.useState(false);

  React.useEffect(() => {
    return () => {
      cancelRef.current.cancelled = true;
      releaseProgressUnsub();
    };
  }, [releaseProgressUnsub]);

  React.useEffect(() => {
    if (task?.status !== 'running') {
      return;
    }
    setElapsedSec(0);
    const t0 = Date.now();
    const id = window.setInterval(() => {
      setElapsedSec(Math.floor((Date.now() - t0) / 1000));
    }, 1000);
    return () => window.clearInterval(id);
  }, [task?.status, task?.action]);

  const envCheck = checkChannelEnv(selectedChannel, runtimes);
  const envPlan = resolveAutoInstallPlan(runtimes, [
    ...envCheck.missing,
    ...envCheck.outdated,
    ...envCheck.broken,
  ]);
  const canOneClickEnv = envPlan.targets.length > 0;
  const busy = task?.status === 'running' || uninstalling;

  const runInstallOutcome = async (
    action: AgentCardTask['action'],
    command: string,
    run: () => Promise<{ ok: boolean; logs: string[]; message: string }>,
    onOk: () => void,
  ) => {
    cancelRef.current = { cancelled: false };
    setElapsedSec(0);
    setTask({
      action,
      command,
      lines: [
        '正在执行…',
        '# 下载/安装可能需数分钟；有新输出时会实时显示在下方',
      ],
      status: 'running',
    });
    setShowEnvPanel(false);

    releaseProgressUnsub();
    const agentId = agent.agentId;
    void onInstallProgress((payload) => {
      if (cancelRef.current.cancelled) return;
      if (payload.action === 'runtime') {
        if (action !== 'oneclick') return;
      } else if (!isProgressForAgent(payload, agentId)) {
        return;
      }
      const line = payload.line?.trimEnd();
      if (!line) return;
      setTask((prev) => {
        if (!prev || prev.status !== 'running') return prev;
        const base =
          prev.lines.length === 2 && prev.lines[0] === '正在执行…'
            ? []
            : prev.lines;
        const next = [...base, line];
        return {
          ...prev,
          lines: next.length > 400 ? next.slice(next.length - 400) : next,
        };
      });
    }).then((unsub) => {
      progressUnsubRef.current =
        typeof unsub === 'function' ? () => { unsub(); } : null;
    });

    try {
      const outcome = await run();
      if (cancelRef.current.cancelled) return;
      setTask({
        action,
        command,
        lines: outcome.logs.length ? outcome.logs : [outcome.message],
        status: outcome.ok ? 'done' : 'failed',
      });
      if (outcome.ok) {
        onOk();
        await new Promise((r) => setTimeout(r, DONE_HOLD_MS));
      } else {
        toast({ title: '操作未成功', description: outcome.message, variant: 'danger' });
      }
    } catch (e) {
      if (e instanceof InstallFailedError) {
        setTask({
          action,
          command,
          lines: e.logs.length ? e.logs : [e.message],
          status: 'failed',
        });
        toast({ title: '操作失败', description: e.message, variant: 'danger' });
        return;
      }
      setTask((prev) => (prev ? { ...prev, status: 'failed', lines: [String(e)] } : prev));
      toast({ title: '操作失败', description: String(e), variant: 'danger' });
    } finally {
      releaseProgressUnsub();
    }
  };

  const startAgentInstall = (channel: InstallChannelMeta) => {
    const check = checkChannelEnv(channel, runtimes);
    if (!check.ready) {
      setSelectedChannelId(channel.id);
      setEnvAutoStart(canOneClickEnv);
      setShowEnvPanel(true);
      toast({
        title: '环境未就绪',
        description: canOneClickEnv
          ? `将一键安装 ${envPlan.summary}`
          : `请先处理: ${formatMissingList([...check.missing, ...check.outdated, ...check.broken])}`,
        variant: canOneClickEnv ? 'default' : 'danger',
      });
      return;
    }

    void runInstallOutcome(
      'install',
      channel.command,
      () => installAgentDetailed(agent.agentId, channel.id, { installDeps: false }),
      () => {
        onChanged();
        toast({ title: `${agentName} 安装完成`, variant: 'success' });
      },
    );
  };

  const startOneClickFull = () => {
    if (!canOneClickEnv && !envCheck.ready) {
      setShowEnvPanel(true);
      setEnvAutoStart(false);
      toast({
        title: '无法一键安装',
        description: `需手动处理: ${formatMissingList([
          ...envCheck.missing,
          ...envCheck.outdated,
          ...envCheck.broken,
        ])}`,
        variant: 'danger',
      });
      return;
    }

    void runInstallOutcome(
      'oneclick',
      `agenthub agent install ${agent.agentId} --install-deps`,
      () =>
        installAgentDetailed(agent.agentId, selectedChannel.id, {
          installDeps: true,
        }),
      () => {
        onChanged();
        onEnvChanged();
        toast({
          title: `${agentName} 一键安装完成`,
          description: envCheck.ready ? undefined : `已尝试安装依赖 ${envPlan.summary}`,
          variant: 'success',
        });
      },
    );
  };

  const startOneClickEnvOnly = () => {
    if (!canOneClickEnv) {
      setShowEnvPanel(true);
      setEnvAutoStart(false);
      return;
    }
    setEnvAutoStart(true);
    setShowEnvPanel(true);
  };

  const startUpgrade = () => {
    void runInstallOutcome(
      'upgrade',
      `$ agenthub agent upgrade ${agent.agentId}`,
      () => upgradeAgentDetailed(agent.agentId),
      () => {
        onChanged();
        onRecheckUpdate?.();
        toast({ title: `${agentName} 已升级`, variant: 'success' });
      },
    );
  };

  const doUninstall = async (deleteConfig: boolean) => {
    setUninstalling(true);
    try {
      const outcome = await uninstallAgentDetailed(agent.agentId, deleteConfig);
      if (!outcome.ok) {
        toast({
          title: '卸载未完成',
          description: outcome.message,
          variant: 'danger',
        });
        setTask({
          action: 'install',
          command: `$ agenthub agent uninstall ${agent.agentId}`,
          lines: outcome.logs,
          status: 'failed',
        });
        return;
      }
      setConfirmDialog(null);
      setConfirmName('');
      onChanged();
      toast({
        title: `${agentName} 已卸载`,
        description: deleteConfig
          ? '配置已处理(未卸载 Node 等共享环境)'
          : undefined,
        variant: 'success',
      });
    } catch (e) {
      const msg = e instanceof InstallFailedError ? e.message : String(e);
      toast({ title: '卸载失败', description: msg, variant: 'danger' });
    } finally {
      setUninstalling(false);
    }
  };

  return {
    task,
    setTask,
    elapsedSec,
    confirmDialog,
    setConfirmDialog,
    confirmName,
    setConfirmName,
    uninstalling,
    showEnvPanel,
    setShowEnvPanel,
    envAutoStart,
    setEnvAutoStart,
    envCheck,
    envPlan,
    canOneClickEnv,
    busy,
    startAgentInstall,
    startOneClickFull,
    startOneClickEnvOnly,
    startUpgrade,
    doUninstall,
    toast,
  };
}
