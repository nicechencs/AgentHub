import * as React from 'react';
import {
  ArrowUpCircle,
  ChevronDown,
  Copy,
  FolderOpen,
  Wrench,
  X,
  Zap,
} from 'lucide-react';
import { AgentLogo } from '@/components/shared/AgentLogo';
import { EnvRemediationPanel } from '@/components/shared/EnvRemediationPanel';
import { InlineTerminal, type TerminalStatus } from '@/components/shared/InlineTerminal';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Input } from '@/components/ui/input';
import { useToast } from '@/components/ui/toast';
import { Hint } from '@/components/ui/tooltip';
import { AGENT_MAP, type InstallChannelMeta } from '@/config/agents';
import {
  installAgentDetailed,
  InstallFailedError,
  openAgentConfig,
  uninstallAgentDetailed,
  upgradeAgentDetailed,
} from '@/lib/api/agent';
import { resolveAutoInstallPlan } from '@/lib/api/env';
import { openExternalLink } from '@/lib/open-external';
import { openPathInFileManager } from '@/lib/api/skill';
import {
  isProgressForAgent,
  onInstallProgress,
} from '@/lib/backend/tauri/install-events';
import { Tip } from '@/components/ui/tooltip';
import { checkChannelEnv, formatMissingList } from '@/lib/env';
import { normalizeOpenPath } from '@/lib/path-open';
import type { AgentStatus, RuntimeDetect } from '@/lib/types';
import { cn } from '@/lib/utils';
import { buildAgentInstallPreview, buildEnvInstallPreview } from './install-preview';

/** Prefer probe `setupUrl`; fall back to native channel when it is a bare https URL. */
function resolveOfficialSetupUrl(
  updateSetupUrl: string | undefined,
  channels: InstallChannelMeta[],
): string | undefined {
  const fromProbe = updateSetupUrl?.trim();
  if (fromProbe && /^https:\/\//i.test(fromProbe)) return fromProbe;
  for (const ch of channels) {
    if (ch.id !== 'native') continue;
    const cmd = ch.command?.trim();
    if (cmd && /^https:\/\//i.test(cmd)) return cmd;
  }
  return undefined;
}

/**
 * Format CLI version for UI: strip name noise so `codex-cli 0.144.5` → `v0.144.5`
 * (avoids the broken `vcodex-cli 0.144.5` when prefixing a raw `v`).
 */
function formatAgentVersion(raw?: string | null): string | undefined {
  if (!raw?.trim()) return undefined;
  const s = raw.trim();
  const token =
    s
      .split(/[\s()]+/)
      .map((p) => p.trim())
      .filter(Boolean)
      .find((p) => {
        const t = p.replace(/^[vV]/, '');
        return t.length > 0 && /^\d/.test(t);
      }) ?? s;
  const cleaned = token.replace(/^[vV]/, '').replace(/[,;)]+$/g, '');
  if (!cleaned || !/\d/.test(cleaned)) return s;
  return `v${cleaned}`;
}

interface Task {
  action: 'install' | 'upgrade' | 'oneclick';
  command: string;
  lines: string[];
  status: TerminalStatus;
}

const DONE_HOLD_MS = 500;

export function AgentCard({
  agent,
  runtimes,
  onChanged,
  onEnvChanged,
  onRecheckUpdate,
}: {
  agent: AgentStatus;
  runtimes: RuntimeDetect[];
  onChanged: () => void;
  onEnvChanged: () => void;
  /** After upgrade, parent may force-refresh update probe for this agent. */
  onRecheckUpdate?: () => void;
}) {
  const { toast } = useToast();
  const meta = AGENT_MAP[agent.agentId];
  const [selectedChannelId, setSelectedChannelId] = React.useState(
    () => agent.channel ?? meta?.installChannels[0]?.id ?? 'native',
  );
  const [task, setTask] = React.useState<Task | null>(null);
  const cancelRef = React.useRef({ cancelled: false });
  const [elapsedSec, setElapsedSec] = React.useState(0);
  const progressUnsubRef = React.useRef<(() => void) | null>(null);
  const releaseProgressUnsub = React.useCallback(() => {
    const stop = progressUnsubRef.current;
    progressUnsubRef.current = null;
    if (typeof stop === 'function') stop();
  }, []);

  const [confirmDialog, setConfirmDialog] = React.useState<
    'program' | 'config' | 'force-upgrade' | null
  >(null);
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

  // Live elapsed timer while install/upgrade task is running.
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

  if (!meta) {
    return (
      <Card className="min-h-20 p-3 text-sm text-muted">
        未知 Agent：{agent.agentId}
      </Card>
    );
  }

  const selectedChannel: InstallChannelMeta | undefined =
    meta.installChannels.find((c) => c.id === selectedChannelId) ?? meta.installChannels[0];

  if (!selectedChannel) {
    return (
      <Card className="min-h-20 p-3 text-sm text-muted">
        {meta.name}：安装渠道加载中…
      </Card>
    );
  }

  const envCheck = checkChannelEnv(selectedChannel, runtimes);
  const envPlan = resolveAutoInstallPlan(runtimes, [
    ...envCheck.missing,
    ...envCheck.outdated,
    ...envCheck.broken,
  ]);
  const canOneClickEnv = envPlan.targets.length > 0;

  const updateState = agent.update?.state;
  const checkingUpdate = updateState === 'checking';
  // Prefer structured update probe; fall back to legacy latestVersion compare.
  const upgradable =
    agent.installed &&
    (updateState === 'update_available' ||
      (!agent.update &&
        !!agent.latestVersion &&
        !!agent.version &&
        agent.latestVersion !== agent.version));
  const updateUnsupported = updateState === 'unsupported';
  const officialSetupUrl = resolveOfficialSetupUrl(
    agent.update?.setupUrl,
    meta.installChannels,
  );
  const canForceUpgrade = agent.installed && !updateUnsupported;
  const latestLabel =
    agent.update?.latestVersion ?? agent.latestVersion ?? undefined;
  const versionLabel = formatAgentVersion(agent.version);
  const latestVersionLabel = formatAgentVersion(latestLabel);

  const openOfficialSetup = () => {
    if (!officialSetupUrl) {
      toast({
        title: '暂无官网下载地址',
        description: agent.update?.note ?? '请到该 Agent 官网手动更新',
        variant: 'danger',
      });
      return;
    }
    void (async () => {
      try {
        await openExternalLink(officialSetupUrl);
      } catch (e) {
        toast({
          title: '无法打开官网',
          description: e instanceof Error ? e.message : String(e),
          variant: 'danger',
        });
      }
    })();
  };

  const cardState: 'installed' | 'ready_to_install' | 'env_missing' = agent.installed
    ? 'installed'
    : envCheck.ready
      ? 'ready_to_install'
      : 'env_missing';

  const busy = task?.status === 'running' || uninstalling;

  /** 统一走 *Detailed port：Tauri 与 Mock 返回同一 InstallOutcome 契约 */
  const runInstallOutcome = async (
    action: Task['action'],
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

    // Subscribe to Tauri install-progress for live lines (no-op in mock/browser).
    releaseProgressUnsub();
    const agentId = agent.agentId;
    void onInstallProgress((payload) => {
      if (cancelRef.current.cancelled) return;
      // Runtime install lines only during one-click full install on this card.
      if (payload.action === 'runtime') {
        if (action !== 'oneclick') return;
      } else if (!isProgressForAgent(payload, agentId)) {
        return;
      }
      const line = payload.line?.trimEnd();
      if (!line) return;
      setTask((prev) => {
        if (!prev || prev.status !== 'running') return prev;
        // Drop the static placeholder once real output arrives.
        const base =
          prev.lines.length === 2 && prev.lines[0] === '正在执行…'
            ? []
            : prev.lines;
        // Cap live buffer to keep UI snappy.
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
        toast({ title: `${meta.name} 安装完成`, variant: 'success' });
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
          title: `${meta.name} 一键安装完成`,
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
        toast({ title: `${meta.name} 已升级`, variant: 'success' });
      },
    );
  };

  const onUpgradeClick = () => {
    if (upgradable) {
      startUpgrade();
      return;
    }
    // 已最新 / 未知：二次确认后再强制重装
    setConfirmDialog('force-upgrade');
  };

  const updateSource = agent.update?.source;
  const sourceHint =
    updateSource && updateSource !== 'npm' && updateSource !== 'none'
      ? ` · ${updateSource}`
      : '';

  const upgradeTooltip = (() => {
    if (checkingUpdate) return '正在检查更新…';
    if (updateUnsupported) {
      const note = agent.update?.note ?? '该 Agent 不支持自动更新检测';
      return officialSetupUrl
        ? `${note} · 点击打开官网下载`
        : note;
    }
    if (upgradable) {
      return latestVersionLabel
        ? `可更新到 ${latestVersionLabel}${sourceHint}`
        : '检测到可用更新';
    }
    if (updateState === 'up_to_date') {
      return latestVersionLabel
        ? `已是最新 ${latestVersionLabel}${sourceHint} · 点击可强制重装`
        : '已是最新 · 点击可强制重装';
    }
    if (updateState === 'unknown') {
      // Prefer structured note; still surface remote version when probe partially succeeded.
      if (agent.update?.note) {
        return latestVersionLabel
          ? `${agent.update.note}（远端 ${latestVersionLabel}）`
          : agent.update.note;
      }
      return latestVersionLabel
        ? `远端 ${latestVersionLabel} · 未能严格比对 · 点击可强制升级`
        : '未能检测更新 · 点击可强制升级';
    }
    return '强制升级到最新（按已装渠道重装 / 重跑官方脚本）';
  })();

  const copyCommand = () => {
    if (!task) return;
    navigator.clipboard.writeText(task.command).catch(() => {});
    toast({ title: '命令已复制' });
  };

  /** Open agent config home (~/.claude 等) in the OS file manager. */
  const openConfigDir = () => {
    void (async () => {
      try {
        const path = await openAgentConfig(agent.agentId);
        toast({
          title: '已打开配置目录',
          description: path ?? meta.name,
          variant: 'success',
        });
      } catch (e) {
        toast({
          title: '打开失败',
          description: e instanceof Error ? e.message : String(e),
          variant: 'danger',
        });
      }
    })();
  };

  /** Open parent folder of the installed binary when path is known. */
  const openBinDir = () => {
    void (async () => {
      const bin = normalizeOpenPath(agent.binPath);
      if (!bin) {
        toast({ title: '没有可打开的安装路径', variant: 'danger' });
        return;
      }
      // Prefer the binary's parent directory; fall back to the path itself.
      const sep = bin.includes('\\') ? '\\' : '/';
      const last = bin.lastIndexOf(sep);
      const dir = last > 2 ? bin.slice(0, last) : bin;
      try {
        await openPathInFileManager(dir);
        toast({ title: '已打开安装目录', description: dir, variant: 'success' });
      } catch (e) {
        toast({
          title: '打开失败',
          description: e instanceof Error ? e.message : String(e),
          variant: 'danger',
        });
      }
    })();
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
        title: `${meta.name} 已卸载`,
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

  return (
    <Card
      className={cn(
        'min-h-20 p-3',
        cardState === 'env_missing' && 'border-warning/35',
      )}
    >
      <div className="flex items-start justify-between gap-4">
        <div className="flex min-w-0 items-start gap-3">
          <AgentLogo agentId={agent.agentId} size="lg" />
          <div className="min-w-0">
            <div className="flex flex-wrap items-center gap-2">
              <span className="font-medium">{meta.name}</span>
              {agent.installed ? (
                <>
                  {versionLabel && (
                    <span className="text-sm text-secondary">{versionLabel}</span>
                  )}
                  {upgradable && latestVersionLabel && (
                    <span className="text-xs text-success">
                      最新 {latestVersionLabel} ↗
                    </span>
                  )}
                  {updateState === 'up_to_date' && (
                    <span className="text-xs text-muted">已最新</span>
                  )}
                  {updateState === 'unknown' && (
                    <span className="text-xs text-muted">更新未知</span>
                  )}
                  {updateUnsupported &&
                    (officialSetupUrl ? (
                      <button
                        type="button"
                        disabled={busy}
                        onClick={openOfficialSetup}
                        className="cursor-pointer text-xs text-accent underline-offset-2 hover:underline disabled:cursor-not-allowed disabled:opacity-60"
                        title={`打开官网下载：${officialSetupUrl}`}
                      >
                        需官网更新
                      </button>
                    ) : (
                      <span
                        className="text-xs text-muted"
                        title={agent.update?.note ?? '该 Agent 不支持自动更新检测'}
                      >
                        需官网更新
                      </span>
                    ))}
                </>
              ) : cardState === 'env_missing' ? (
                <>
                  <span className="text-sm text-muted">未安装</span>
                  <Badge variant="warning">环境未就绪</Badge>
                  {canOneClickEnv && <Badge>可一键安装</Badge>}
                </>
              ) : (
                <>
                  <span className="text-sm text-muted">未安装</span>
                  <Badge variant="success">环境就绪</Badge>
                </>
              )}
            </div>

            {agent.installed ? (
              <div className="mt-1 flex flex-wrap items-center gap-2 font-mono text-xs text-muted">
                <span className="truncate">{agent.binPath}</span>
                {agent.channel && <Badge>{agent.channel}</Badge>}
              </div>
            ) : (
              <div className="mt-1 space-y-1 text-xs text-muted">
                <div className="flex flex-wrap items-center gap-2">
                  <span>渠道</span>
                  <div className="flex flex-wrap gap-1">
                    {meta.installChannels.map((ch) => {
                      const chCheck = checkChannelEnv(ch, runtimes);
                      const active = ch.id === selectedChannel.id;
                      return (
                        <Hint
                          key={ch.id}
                          label={!chCheck.ready ? '该渠道环境未就绪' : ch.label}
                        >
                          <button
                            type="button"
                            disabled={busy}
                            onClick={() => {
                              setSelectedChannelId(ch.id);
                              setShowEnvPanel(false);
                            }}
                            className={cn(
                              'cursor-pointer rounded-full border px-2 py-0.5 transition-colors',
                              active
                                ? 'border-accent/40 bg-accent/10 font-medium text-accent'
                                : 'border-border bg-subtle text-secondary hover:bg-hover hover:text-primary',
                            )}
                          >
                            {ch.id}
                          </button>
                        </Hint>
                      );
                    })}
                  </div>
                </div>
                {cardState === 'env_missing' && (
                  <Tip
                    className="truncate text-secondary"
                    label={`缺少: ${formatMissingList([
                      ...envCheck.missing,
                      ...envCheck.outdated,
                      ...envCheck.broken,
                    ])}${canOneClickEnv ? ` · 一键可装 ${envPlan.summary}` : ''}`}
                  >
                    缺少{' '}
                    {formatMissingList([
                      ...envCheck.missing,
                      ...envCheck.outdated,
                      ...envCheck.broken,
                    ])}
                  </Tip>
                )}
              </div>
            )}
          </div>
        </div>

        <div className="flex shrink-0 items-center justify-end gap-1.5">
          {agent.installed ? (
            <>
              <Button
                size="icon"
                variant={upgradable ? 'default' : 'secondary'}
                className={
                  upgradable
                    ? 'bg-success text-white hover:bg-success/90 focus-visible:ring-success/60'
                    : undefined
                }
                disabled={
                  busy ||
                  checkingUpdate ||
                  (updateUnsupported
                    ? !officialSetupUrl
                    : !upgradable && !canForceUpgrade)
                }
                aria-label={
                  updateUnsupported
                    ? '打开官网更新'
                    : upgradable
                      ? '更新'
                      : '强制升级'
                }
                title={upgradeTooltip}
                onClick={updateUnsupported ? openOfficialSetup : onUpgradeClick}
              >
                <ArrowUpCircle
                  className={cn(
                    'h-3.5 w-3.5',
                    checkingUpdate && 'animate-pulse opacity-70',
                  )}
                />
              </Button>
              <Button
                size="icon"
                variant="outline"
                disabled={busy}
                aria-label="打开配置目录"
                title="打开该 Agent 的配置目录"
                onClick={openConfigDir}
              >
                <FolderOpen className="h-3.5 w-3.5" />
              </Button>
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button size="icon" variant="outline" disabled={busy} aria-label="更多">
                    ···
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end">
                  {agent.binPath?.trim() ? (
                    <>
                      <DropdownMenuItem
                        onSelect={() => {
                          void openBinDir();
                        }}
                      >
                        <FolderOpen className="h-3.5 w-3.5" /> 打开安装目录
                      </DropdownMenuItem>
                      <DropdownMenuSeparator />
                    </>
                  ) : null}
                  <DropdownMenuItem onSelect={() => setConfirmDialog('program')}>
                    仅卸载程序
                  </DropdownMenuItem>
                  <DropdownMenuItem className="text-danger" onSelect={() => setConfirmDialog('config')}>
                    卸载并删除配置
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
            </>
          ) : cardState === 'env_missing' ? (
            <>
              <Button
                size="sm"
                variant="secondary"
                onClick={canOneClickEnv ? startOneClickFull : startOneClickEnvOnly}
                disabled={busy}
                title={
                  canOneClickEnv
                    ? '先修环境再安装当前渠道 Agent'
                    : '仅安装缺失的运行环境，装完后再装 Agent'
                }
              >
                <Zap className="h-3.5 w-3.5" />
                {canOneClickEnv ? '修复并安装' : '修环境'}
              </Button>
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button size="icon" variant="outline" disabled={busy} aria-label="更多">
                    ···
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end">
                  <DropdownMenuItem onSelect={startOneClickEnvOnly}>
                    <Wrench className="h-3.5 w-3.5" /> 仅修环境
                  </DropdownMenuItem>
                  <DropdownMenuItem
                    onSelect={() => {
                      const lines =
                        envPlan.targets.length > 0
                          ? buildEnvInstallPreview(envPlan.targets, 'winget')
                          : buildAgentInstallPreview(agent.agentId, 'install', selectedChannel.id);
                      navigator.clipboard.writeText(lines.join('\n')).catch(() => {});
                      toast({ title: '命令预览已复制' });
                    }}
                  >
                    <Copy className="h-3.5 w-3.5" /> 复制命令
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
            </>
          ) : (
            <>
              <Button
                size="sm"
                variant="secondary"
                onClick={() => startAgentInstall(selectedChannel)}
                disabled={busy}
                title={`使用渠道 ${selectedChannel.id} 安装`}
              >
                <Zap className="h-3.5 w-3.5" />
                安装
              </Button>
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button size="sm" variant="outline" disabled={busy}>
                    渠道 <ChevronDown className="h-3 w-3" />
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end">
                  {meta.installChannels.map((ch) => {
                    const chCheck = checkChannelEnv(ch, runtimes);
                    return (
                      <DropdownMenuItem
                        key={ch.id}
                        onSelect={() => {
                          setSelectedChannelId(ch.id);
                          startAgentInstall(ch);
                        }}
                      >
                        {ch.label}
                        {!chCheck.ready ? ' · 需先修环境' : ''}
                      </DropdownMenuItem>
                    );
                  })}
                </DropdownMenuContent>
              </DropdownMenu>
            </>
          )}
        </div>
      </div>

      {showEnvPanel && cardState === 'env_missing' && (
        <div className="mt-3">
          <EnvRemediationPanel
            key={`card-env-${agent.agentId}-${envAutoStart}`}
            compact
            runtime={runtimes.find((r) => envCheck.missing.includes(r.id) || envCheck.outdated.includes(r.id))}
            runtimes={runtimes}
            focusIds={[...envCheck.missing, ...envCheck.outdated, ...envCheck.broken]}
            autoStart={envAutoStart}
            onDismiss={() => {
              setShowEnvPanel(false);
              setEnvAutoStart(false);
            }}
            onDone={() => {
              setShowEnvPanel(false);
              setEnvAutoStart(false);
              onEnvChanged();
            }}
          />
        </div>
      )}

      {task && (
        <div className="mt-3">
          <div className="mb-1 flex items-center justify-between">
            <span className="text-xs text-muted">
              {task.action === 'oneclick'
                ? '环境 → Agent'
                : task.action === 'install'
                  ? '安装中'
                  : '升级中'}
            </span>
            <div className="flex items-center gap-1">
              <Button size="sm" variant="ghost" onClick={copyCommand} title="复制 CLI 命令">
                <Copy className="h-3.5 w-3.5" /> 复制
              </Button>
              {task.status !== 'running' && (
                <Button size="sm" variant="ghost" onClick={() => setTask(null)}>
                  <X className="h-3.5 w-3.5" /> 关闭
                </Button>
              )}
            </div>
          </div>
          <InlineTerminal
            lines={task.lines}
            status={task.status}
            elapsedSec={task.status === 'running' ? elapsedSec : undefined}
          />
        </div>
      )}

      <Dialog open={confirmDialog === 'program'} onOpenChange={(o) => !o && setConfirmDialog(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>卸载 {meta.name}？</DialogTitle>
            <DialogDescription>
              只卸程序，不卸 Node 等共享环境。
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setConfirmDialog(null)} disabled={uninstalling}>
              取消
            </Button>
            <Button variant="danger" onClick={() => void doUninstall(false)} disabled={uninstalling}>
              {uninstalling ? '卸载中...' : '确认卸载'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog
        open={confirmDialog === 'force-upgrade'}
        onOpenChange={(o) => !o && setConfirmDialog(null)}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>强制升级 {meta.name}？</DialogTitle>
            <DialogDescription>
              {updateState === 'up_to_date'
                ? '当前检测为已是最新版本。强制升级将按已装渠道重新安装 / 重跑官方脚本。'
                : '未能确认是否有新版本。强制升级将按已装渠道重新安装 / 重跑官方脚本。'}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setConfirmDialog(null)} disabled={busy}>
              取消
            </Button>
            <Button
              variant="default"
              disabled={busy}
              onClick={() => {
                setConfirmDialog(null);
                startUpgrade();
              }}
            >
              确认强制升级
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog
        open={confirmDialog === 'config'}
        onOpenChange={(o) => {
          if (!o) {
            setConfirmDialog(null);
            setConfirmName('');
          }
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>卸载并删除配置？</DialogTitle>
            <DialogDescription className="text-danger">
              先备份再删配置；不卸 Node 等共享环境。
            </DialogDescription>
          </DialogHeader>
          <div className="text-sm text-secondary">
            输入 <span className="font-mono font-medium text-primary">{meta.name}</span> 确认：
          </div>
          <Input
            value={confirmName}
            onChange={(e) => setConfirmName(e.target.value)}
            placeholder={meta.name}
            className="mt-2"
            disabled={uninstalling}
          />
          <DialogFooter>
            <Button variant="outline" onClick={() => setConfirmDialog(null)} disabled={uninstalling}>
              取消
            </Button>
            <Button
              variant="danger"
              onClick={() => void doUninstall(true)}
              disabled={uninstalling || confirmName !== meta.name}
            >
              {uninstalling ? '卸载中...' : '卸载并删除配置'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </Card>
  );
}
