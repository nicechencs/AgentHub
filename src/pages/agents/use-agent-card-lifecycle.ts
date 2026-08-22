import * as React from 'react';
import type { TerminalStatus } from '@/components/shared/InlineTerminal';
import { useI18n } from '@/components/shared/LanguageProvider';
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

type ProgressSubscribe = (
  handler: Parameters<typeof onInstallProgress>[0],
) => Promise<unknown>;
type Unsubscribe = () => void;

function isUnsubscribe(value: unknown): value is Unsubscribe {
  return typeof value === 'function';
}

const INSTALL_OUTPUT_LINE_CAP = 400;

/** Append a raw UTF-8 install chunk. Empty / whitespace / newline-only stay. */
export function recordInstallOutputChunk(chunks: string[], chunk: string): string[] {
  chunks.push(chunk);
  return chunks;
}

/** Join raw chunks then split on `\n` so mid-line splits are not extra rows. */
export function installOutputChunksToLines(chunks: string[]): string[] {
  const lines = chunks.join('').split('\n');
  return lines.length > INSTALL_OUTPUT_LINE_CAP
    ? lines.slice(lines.length - INSTALL_OUTPUT_LINE_CAP)
    : lines;
}

/**
 * Keep the unsubscribe local to one install attempt. If the async listener
 * setup resolves after disposal, its late unsubscribe is invoked immediately
 * instead of leaking a listener into the next attempt.
 */
export function createInstallProgressSubscription(
  subscribe: ProgressSubscribe,
  handler: Parameters<ProgressSubscribe>[0],
) {
  let disposed = false;
  let unsubscribe: Unsubscribe | null = null;

  const dispose = () => {
    disposed = true;
    const stop = unsubscribe;
    unsubscribe = null;
    stop?.();
  };

  const ready = Promise.resolve()
    .then(() => subscribe(handler))
    .then((candidate) => {
      if (!isUnsubscribe(candidate)) {
        throw new Error('install progress subscription did not return an unsubscribe function');
      }
      if (disposed) {
        candidate();
        return;
      }
      unsubscribe = candidate;
    });

  return { ready, dispose };
}

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
  const { t } = useI18n();
  const { toast } = useToast();
  const [task, setTask] = React.useState<AgentCardTask | null>(null);
  const cancelRef = React.useRef({ cancelled: false });
  const [elapsedSec, setElapsedSec] = React.useState(0);
  const progressUnsubRef = React.useRef<(() => void) | null>(null);
  const liveOutputChunksRef = React.useRef<string[]>([]);
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
    cancelRef.current.cancelled = true;
    const cancelToken = { cancelled: false };
    cancelRef.current = cancelToken;
    setElapsedSec(0);
    const executingLine = t('agents.lifecycle.executing');
    setTask({
      action,
      command,
      lines: [
        executingLine,
        t('agents.lifecycle.executingHint'),
      ],
      status: 'running',
    });
    setShowEnvPanel(false);

    releaseProgressUnsub();
    liveOutputChunksRef.current = [];
    const agentId = agent.agentId;
    const progressSubscription = createInstallProgressSubscription(onInstallProgress, (payload) => {
      if (cancelToken.cancelled) return;
      if (payload.action === 'runtime') {
        if (action !== 'oneclick') return;
      } else if (!isProgressForAgent(payload, agentId)) {
        return;
      }
      if (typeof payload.line !== 'string') return;
      recordInstallOutputChunk(liveOutputChunksRef.current, payload.line);
      const lines = installOutputChunksToLines(liveOutputChunksRef.current);
      setTask((prev) => {
        if (!prev || prev.status !== 'running') return prev;
        return {
          ...prev,
          lines,
        };
      });
    });
    progressUnsubRef.current = progressSubscription.dispose;

    try {
      await progressSubscription.ready;
      if (cancelToken.cancelled) return;
      const outcome = await run();
      if (cancelToken.cancelled) return;
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
        toast({ title: t('agents.lifecycle.notOk'), description: outcome.message, variant: 'danger' });
      }
    } catch (e) {
      if (cancelToken.cancelled) return;
      if (e instanceof InstallFailedError) {
        setTask({
          action,
          command,
          lines: e.logs.length ? e.logs : [e.message],
          status: 'failed',
        });
        toast({ title: t('agents.lifecycle.failed'), description: e.message, variant: 'danger' });
        return;
      }
      setTask((prev) => (prev ? { ...prev, status: 'failed', lines: [String(e)] } : prev));
      toast({ title: t('agents.lifecycle.failed'), description: String(e), variant: 'danger' });
    } finally {
      progressSubscription.dispose();
      if (progressUnsubRef.current === progressSubscription.dispose) {
        progressUnsubRef.current = null;
      }
    }
  };

  const startAgentInstall = (channel: InstallChannelMeta) => {
    const check = checkChannelEnv(channel, runtimes);
    if (!check.ready) {
      setSelectedChannelId(channel.id);
      setEnvAutoStart(canOneClickEnv);
      setShowEnvPanel(true);
      toast({
        title: t('agents.lifecycle.envNotReady'),
        description: canOneClickEnv
          ? t('agents.lifecycle.willOneClick', { summary: envPlan.summary })
          : t('agents.lifecycle.handleFirst', {
              list: formatMissingList([...check.missing, ...check.outdated, ...check.broken]),
            }),
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
        toast({ title: t('agents.lifecycle.installDone', { name: agentName }), variant: 'success' });
      },
    );
  };

  const startOneClickFull = () => {
    if (!canOneClickEnv && !envCheck.ready) {
      setShowEnvPanel(true);
      setEnvAutoStart(false);
      toast({
        title: t('agents.lifecycle.cannotOneClick'),
        description: t('agents.lifecycle.needManual', {
          list: formatMissingList([
            ...envCheck.missing,
            ...envCheck.outdated,
            ...envCheck.broken,
          ]),
        }),
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
          title: t('agents.lifecycle.oneClickDone', { name: agentName }),
          description: envCheck.ready
            ? undefined
            : t('agents.lifecycle.triedDeps', { summary: envPlan.summary }),
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
        toast({ title: t('agents.lifecycle.upgraded', { name: agentName }), variant: 'success' });
      },
    );
  };

  const doUninstall = async (deleteConfig: boolean) => {
    setUninstalling(true);
    try {
      const outcome = await uninstallAgentDetailed(agent.agentId, deleteConfig);
      if (!outcome.ok) {
        toast({
          title: t('agents.lifecycle.uninstallIncomplete'),
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
        title: t('agents.lifecycle.uninstalled', { name: agentName }),
        description: deleteConfig
          ? t('agents.lifecycle.configHandled')
          : undefined,
        variant: 'success',
      });
    } catch (e) {
      const msg = e instanceof InstallFailedError ? e.message : String(e);
      toast({ title: t('agents.lifecycle.uninstallFailed'), description: msg, variant: 'danger' });
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
