import * as React from 'react';
import {
  ArrowUpCircle,
  ChevronDown,
  Copy,
  Eye,
  EyeOff,
  FolderOpen,
  Wrench,
  X,
  Zap,
} from 'lucide-react';
import { AgentLogo } from '@/components/shared/AgentLogo';
import { EnvRemediationPanel } from '@/components/shared/EnvRemediationPanel';
import { InlineTerminal } from '@/components/shared/InlineTerminal';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Hint } from '@/components/ui/tooltip';
import { AGENT_MAP, type InstallChannelMeta } from '@/config/agents';
import { openAgentConfig, setAgentHidden } from '@/lib/api/agent';
import { runtimeChannelForPlan } from '@/lib/env-plan';
import { openExternalLink } from '@/lib/open-external';
import { openPathInFileManager } from '@/lib/api/skill';
import { Tip } from '@/components/ui/tooltip';
import { checkChannelEnv, formatMissingList } from '@/lib/env';
import { normalizeOpenPath } from '@/lib/path-open';
import type { AgentStatus, RuntimeDetect } from '@/lib/types';
import { cn } from '@/lib/utils';
import { buildAgentInstallPreview, buildEnvInstallPreview } from './install-preview';
import { formatAgentVersion, resolveOfficialSetupUrl } from './agent-card-model';
import { AgentCardDialogs } from './AgentCardDialogs';
import { useAgentCardLifecycle } from './use-agent-card-lifecycle';

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
  const meta = AGENT_MAP[agent.agentId];
  const [selectedChannelId, setSelectedChannelId] = React.useState(
    () => agent.channel ?? meta?.installChannels[0]?.id ?? 'native',
  );

  const selectedChannelFallback: InstallChannelMeta = meta?.installChannels.find((c) => c.id === selectedChannelId)
    ?? meta?.installChannels[0]
    ?? { id: 'native', label: 'native', command: '', requires: [] };

  const life = useAgentCardLifecycle({
    agent,
    agentName: meta?.name ?? agent.agentId,
    runtimes,
    selectedChannel: selectedChannelFallback,
    selectedChannelId,
    setSelectedChannelId,
    onChanged,
    onEnvChanged,
    onRecheckUpdate,
  });

  const {
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
  } = life;

  const [hiding, setHiding] = React.useState(false);
  const hidden = Boolean(agent.hidden);
  const actionsBusy = busy || hiding;

  const toggleHidden = async () => {
    setHiding(true);
    try {
      await setAgentHidden(agent.agentId, !hidden);
      toast({
        title: hidden ? '已取消隐藏' : '已隐藏',
        description: hidden
          ? `${meta?.name ?? agent.agentId} 已恢复显示`
          : `${meta?.name ?? agent.agentId} 已从其他页面隐藏`,
        variant: 'success',
      });
    } catch (e) {
      toast({
        title: hidden ? '取消隐藏失败' : '隐藏失败',
        description: e instanceof Error ? e.message : String(e),
        variant: 'danger',
      });
    } finally {
      setHiding(false);
    }
  };

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

  const updateState = agent.update?.state;
  const checkingUpdate = updateState === 'checking';
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

  const onUpgradeClick = () => {
    if (upgradable) {
      startUpgrade();
      return;
    }
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
        ? `已是最新 ${latestVersionLabel}${sourceHint} · 点击可强制升级`
        : '已是最新 · 点击可强制升级';
    }
    if (updateState === 'unknown') {
      return agent.update?.note
        ? `${agent.update.note} · 点击可强制升级`
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

  return (

    <Card
      className={cn(
        'min-h-20 p-3',
        cardState === 'env_missing' && !hidden && 'border-warning/35',
        hidden && 'opacity-60 grayscale',
      )}
    >
      <div className="flex items-start justify-between gap-4">
        <div className="flex min-w-0 items-start gap-3">
          <AgentLogo agentId={agent.agentId} size="lg" />
          <div className="min-w-0">
            <div className="flex flex-wrap items-center gap-2">
              <span className="font-medium">{meta.name}</span>
              {hidden && <Badge>已隐藏</Badge>}
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
                      <Hint label="打开官网下载">
                        <button
                          type="button"
                          disabled={actionsBusy || hidden}
                          onClick={openOfficialSetup}
                          className="cursor-pointer text-xs text-accent underline-offset-2 hover:underline disabled:cursor-not-allowed disabled:opacity-60"
                        >
                          需官网更新
                        </button>
                      </Hint>
                    ) : (
                      <Tip
                        className="text-xs text-muted"
                        label={agent.update?.note ?? '该 Agent 不支持自动更新检测'}
                      >
                        需官网更新
                      </Tip>
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
          {hidden ? (
            <Button
              size="sm"
              variant="outline"
              disabled={hiding}
              aria-label="取消隐藏"
              title="取消隐藏后恢复显示与操作"
              onClick={() => void toggleHidden()}
            >
              <Eye className="h-3.5 w-3.5" />
              取消隐藏
            </Button>
          ) : agent.installed ? (
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
              <Button
                size="icon"
                variant="outline"
                disabled={actionsBusy}
                aria-label="隐藏"
                title="隐藏后其他页面不再显示此 Agent"
                onClick={() => void toggleHidden()}
              >
                <EyeOff className="h-3.5 w-3.5" />
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
              <Button
                size="icon"
                variant="outline"
                disabled={actionsBusy}
                aria-label="隐藏"
                title="隐藏后其他页面不再显示此 Agent"
                onClick={() => void toggleHidden()}
              >
                <EyeOff className="h-3.5 w-3.5" />
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
                          ? buildEnvInstallPreview(envPlan.targets, runtimeChannelForPlan())
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
              <Button
                size="icon"
                variant="outline"
                disabled={actionsBusy}
                aria-label="隐藏"
                title="隐藏后其他页面不再显示此 Agent"
                onClick={() => void toggleHidden()}
              >
                <EyeOff className="h-3.5 w-3.5" />
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
            pageHasPrimaryCta
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

      <AgentCardDialogs
        agentName={meta.name}
        confirmDialog={confirmDialog}
        confirmName={confirmName}
        onConfirmNameChange={setConfirmName}
        uninstalling={uninstalling}
        busy={busy}
        updateState={updateState}
        onClose={() => {
          setConfirmDialog(null);
          setConfirmName('');
        }}
        onUninstall={(deleteConfig) => void doUninstall(deleteConfig)}
        onConfirmForceUpgrade={startUpgrade}
      />
    </Card>
  );
}
