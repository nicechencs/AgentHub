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
import { useI18n } from '@/components/shared/LanguageProvider';
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
import { shouldIgnoreMenuDialogDismiss } from '@/pages/connections/ticket-wallet-model';
import { buildAgentInstallPreview, buildEnvInstallPreview } from './install-preview';
import {
  agentTaskLogTitleKey,
  formatAgentVersion,
  isNodeTooOldUpdateNote,
  openAgentCardUninstallConfirm,
  resolveOfficialSetupUrl,
} from './agent-card-model';
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
  const { t } = useI18n();
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
  const ignoreMenuDialogDismissRef = React.useRef(false);

  const openUninstallConfirm = (
    event: { preventDefault: () => void },
    kind: 'program' | 'config',
  ) => {
    openAgentCardUninstallConfirm(event, kind, setConfirmDialog, ignoreMenuDialogDismissRef);
  };

  const toggleHidden = async () => {
    setHiding(true);
    try {
      await setAgentHidden(agent.agentId, !hidden);
      toast({
        title: hidden ? t('agents.env.hiddenOk') : t('agents.env.hidden'),
        description: hidden
          ? t('agents.env.restoredDesc', { name: meta?.name ?? agent.agentId })
          : t('agents.env.hiddenDesc', { name: meta?.name ?? agent.agentId }),
        variant: 'success',
      });
    } catch (e) {
      toast({
        title: hidden ? t('agents.env.unhideFailed') : t('agents.env.hideFailed'),
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
        {t('agents.card.unknown', { id: agent.agentId })}
      </Card>
    );
  }

  const selectedChannel: InstallChannelMeta | undefined =
    meta.installChannels.find((c) => c.id === selectedChannelId) ?? meta.installChannels[0];

  if (!selectedChannel) {
    return (
      <Card className="min-h-20 p-3 text-sm text-muted">
        {t('agents.card.channelLoading', { name: meta.name })}
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
        title: t('agents.update.noOfficialUrl'),
        description: agent.update?.note ?? t('agents.update.manualOfficial'),
        variant: 'danger',
      });
      return;
    }
    void (async () => {
      try {
        await openExternalLink(officialSetupUrl);
      } catch (e) {
        toast({
          title: t('agents.update.cannotOpenOfficial'),
          description: e instanceof Error ? e.message : String(e),
          variant: 'danger',
        });
      }
    })();
  };

  const installFailed = task?.status === 'failed';
  const retryAction = () => {
    if (task?.action === 'upgrade') startUpgrade();
    else if (task?.action === 'oneclick') startOneClickFull();
    else startAgentInstall(selectedChannel);
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
    if (checkingUpdate) return t('agents.update.checking');
    if (updateUnsupported) {
      const note = agent.update?.note ?? t('agents.card.unsupportedUpdate');
      return officialSetupUrl
        ? t('agents.update.clickOfficial', { note })
        : note;
    }
    if (upgradable) {
      return latestVersionLabel
        ? t('agents.update.availableVersion', { version: latestVersionLabel, source: sourceHint })
        : t('agents.update.available');
    }
    if (updateState === 'up_to_date') {
      return latestVersionLabel
        ? t('agents.update.latestForceVersion', { version: latestVersionLabel, source: sourceHint })
        : t('agents.update.latestForce');
    }
    if (updateState === 'unknown') {
      return agent.update?.note
        ? t('agents.update.unknownForceNote', { note: agent.update.note })
        : t('agents.update.unknownForce');
    }
    return t('agents.update.forceLatest');
  })();

  const copyCommand = () => {
    if (!task) return;
    navigator.clipboard.writeText(task.command).catch(() => {});
    toast({ title: t('agents.env.commandCopied') });
  };

  /** Open agent config home (~/.claude 等) in the OS file manager. */
  const openConfigDir = () => {
    void (async () => {
      try {
        const path = await openAgentConfig(agent.agentId);
        toast({
          title: t('agents.env.openedConfigDir'),
          description: path ?? meta.name,
          variant: 'success',
        });
      } catch (e) {
        toast({
          title: t('agents.env.openFailed'),
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
        toast({ title: t('agents.env.noInstallPath'), variant: 'danger' });
        return;
      }
      const sep = bin.includes('\\') ? '\\' : '/';
      const last = bin.lastIndexOf(sep);
      const dir = last > 2 ? bin.slice(0, last) : bin;
      try {
        await openPathInFileManager(dir);
        toast({ title: t('agents.env.openedInstallDir'), description: dir, variant: 'success' });
      } catch (e) {
        toast({
          title: t('agents.env.openFailed'),
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
              {hidden && <Badge>{t('agents.card.hidden')}</Badge>}
              {agent.installed ? (
                <>
                  {versionLabel && (
                    <span className="text-sm text-secondary">{versionLabel}</span>
                  )}
                  {upgradable && latestVersionLabel && (
                    <span className="text-xs text-success">
                      {t('agents.card.latest', { version: latestVersionLabel })}
                    </span>
                  )}
                  {updateState === 'up_to_date' && (
                    <span className="text-xs text-muted">{t('agents.card.upToDate')}</span>
                  )}
                  {updateState === 'unknown' && (
                    <span className="text-xs text-muted">
                      {isNodeTooOldUpdateNote(agent.update?.note)
                        ? t('agents.card.needsNode22')
                        : t('agents.card.updateUnknown')}
                    </span>
                  )}
                  {updateUnsupported &&
                    (officialSetupUrl ? (
                      <Hint label={t('agents.card.openOfficialDownload')}>
                        <button
                          type="button"
                          disabled={actionsBusy || hidden}
                          onClick={openOfficialSetup}
                          className="cursor-pointer text-xs text-accent underline-offset-2 hover:underline disabled:cursor-not-allowed disabled:opacity-60"
                        >
                          {t('agents.card.needsOfficial')}
                        </button>
                      </Hint>
                    ) : (
                      <Tip
                        className="text-xs text-muted"
                        label={agent.update?.note ?? t('agents.card.unsupportedUpdate')}
                      >
                        {t('agents.card.needsOfficial')}
                      </Tip>
                    ))}
                </>
              ) : cardState === 'env_missing' ? (
                <>
                  <span className="text-sm text-muted">{t('agents.card.notInstalled')}</span>
                  <Badge variant="warning">{t('agents.card.envNotReady')}</Badge>
                  {canOneClickEnv && <Badge>{t('agents.card.oneClickInstall')}</Badge>}
                </>
              ) : (
                <>
                  <span className="text-sm text-muted">{t('agents.card.notInstalled')}</span>
                  <Badge variant="success">{t('agents.card.envReady')}</Badge>
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
                  <span>{t('agents.card.channel')}</span>
                  <div className="flex flex-wrap gap-1">
                    {meta.installChannels.map((ch) => {
                      const chCheck = checkChannelEnv(ch, runtimes);
                      const active = ch.id === selectedChannel.id;
                      return (
                        <Hint
                          key={ch.id}
                          label={!chCheck.ready ? t('agents.card.channelEnvNotReady') : ch.label}
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
                    label={
                      canOneClickEnv
                        ? t('agents.card.missingTipOneClick', {
                            list: formatMissingList([
                              ...envCheck.missing,
                              ...envCheck.outdated,
                              ...envCheck.broken,
                            ]),
                            summary: envPlan.summary,
                          })
                        : t('agents.card.missingTip', {
                            list: formatMissingList([
                              ...envCheck.missing,
                              ...envCheck.outdated,
                              ...envCheck.broken,
                            ]),
                          })
                    }
                  >
                    {t('agents.card.missing')}
                    {formatMissingList([
                      ...envCheck.missing,
                      ...envCheck.outdated,
                      ...envCheck.broken,
                    ])}
                  </Tip>
                )}
              </div>
            )}
            {agent.extraCopies && agent.extraCopies.length > 0 ? (
              <div className="mt-1 space-y-0.5 font-mono text-xs text-muted">
                <span>
                  {t('agents.card.extraCopies', { count: String(agent.extraCopies.length) })}
                </span>
                {agent.extraCopies.map((copy) => (
                  <div key={copy.path} className="truncate pl-2">
                    {copy.kind === 'ide'
                      ? t('agents.card.extraCopyIde')
                      : copy.kind === 'desktop'
                        ? t('agents.card.extraCopyDesktop')
                        : copy.kind === 'leftover-agenthub'
                          ? t('agents.card.extraCopyLeftover')
                          : copy.kind}
                    {copy.version ? ` ${copy.version}` : ''}
                    {` · ${copy.path}`}
                  </div>
                ))}
              </div>
            ) : null}
          </div>
        </div>

        <div className="flex shrink-0 items-center justify-end gap-1.5">
          {hidden ? (
            <Button
              size="sm"
              variant="outline"
              disabled={hiding}
              aria-label={t('agents.card.unhide')}
              title={t('agents.card.unhideTitle')}
              onClick={() => void toggleHidden()}
            >
              <Eye className="h-3.5 w-3.5" />
              {t('agents.card.unhide')}
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
                    ? t('agents.card.openOfficialUpdate')
                    : upgradable
                      ? t('agents.card.update')
                      : t('agents.card.forceUpgrade')
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
                aria-label={t('agents.card.openConfigDir')}
                title={t('agents.card.openConfigDirTitle')}
                onClick={openConfigDir}
              >
                <FolderOpen className="h-3.5 w-3.5" />
              </Button>
              <Button
                size="icon"
                variant="outline"
                disabled={actionsBusy}
                aria-label={t('agents.card.hide')}
                title={t('agents.card.hideTitle')}
                onClick={() => void toggleHidden()}
              >
                <EyeOff className="h-3.5 w-3.5" />
              </Button>
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button size="icon" variant="outline" disabled={busy} aria-label={t('agents.card.more')}>
                    ···
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent
                  align="end"
                  onCloseAutoFocus={(event) => event.preventDefault()}
                >
                  {agent.binPath?.trim() ? (
                    <>
                      <DropdownMenuItem
                        onSelect={() => {
                          void openBinDir();
                        }}
                      >
                        <FolderOpen className="h-3.5 w-3.5" /> {t('agents.card.openInstallDir')}
                      </DropdownMenuItem>
                      <DropdownMenuSeparator />
                    </>
                  ) : null}
                  <DropdownMenuItem
                    onSelect={(event) => openUninstallConfirm(event, 'program')}
                  >
                    {t('agents.card.uninstallProgram')}
                  </DropdownMenuItem>
                  <DropdownMenuItem
                    className="text-danger"
                    onSelect={(event) => openUninstallConfirm(event, 'config')}
                  >
                    {t('agents.card.uninstallConfig')}
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
            </>
          ) : cardState === 'env_missing' ? (
            <>
              <Button
                size="sm"
                variant="secondary"
                onClick={canOneClickEnv ? () => setConfirmDialog('oneclick') : startOneClickEnvOnly}
                disabled={busy}
                title={
                  canOneClickEnv
                    ? t('agents.card.fixThenInstall')
                    : t('agents.card.envOnlyThenInstall')
                }
              >
                <Zap className="h-3.5 w-3.5" />
                {canOneClickEnv ? t('agents.card.fixAndInstall') : t('agents.card.fixEnv')}
              </Button>
              <Button
                size="icon"
                variant="outline"
                disabled={actionsBusy}
                aria-label={t('agents.card.hide')}
                title={t('agents.card.hideTitle')}
                onClick={() => void toggleHidden()}
              >
                <EyeOff className="h-3.5 w-3.5" />
              </Button>
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button size="icon" variant="outline" disabled={busy} aria-label={t('agents.card.more')}>
                    ···
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end">
                  <DropdownMenuItem onSelect={startOneClickEnvOnly}>
                    <Wrench className="h-3.5 w-3.5" /> {t('agents.card.envOnly')}
                  </DropdownMenuItem>
                  <DropdownMenuItem
                    onSelect={() => {
                      const lines =
                        envPlan.targets.length > 0
                          ? buildEnvInstallPreview(envPlan.targets, runtimeChannelForPlan())
                          : buildAgentInstallPreview(agent.agentId, 'install', selectedChannel.id);
                      navigator.clipboard.writeText(lines.join('\n')).catch(() => {});
                      toast({ title: t('agents.env.commandPreviewCopied') });
                    }}
                  >
                    <Copy className="h-3.5 w-3.5" /> {t('agents.card.copyCommand')}
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
            </>
          ) : (
            <>
              <Button
                size="sm"
                variant={installFailed ? 'default' : 'secondary'}
                onClick={() => (installFailed ? retryAction() : setConfirmDialog('install'))}
                disabled={busy}
                title={installFailed ? t('agents.card.retry') : t('agents.card.installWithChannel', { id: selectedChannel.id })}
              >
                <Zap className="h-3.5 w-3.5" />
                {installFailed ? t('agents.card.retry') : t('agents.card.install')}
              </Button>
              <Button
                size="icon"
                variant="outline"
                disabled={actionsBusy}
                aria-label={t('agents.card.hide')}
                title={t('agents.card.hideTitle')}
                onClick={() => void toggleHidden()}
              >
                <EyeOff className="h-3.5 w-3.5" />
              </Button>
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button size="sm" variant="outline" disabled={busy}>
                    {t('agents.card.channel')} <ChevronDown className="h-3 w-3" />
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
                          setConfirmDialog('install');
                        }}
                      >
                        {ch.label}
                        {!chCheck.ready ? t('agents.card.needFixEnv') : ''}
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
              {t(agentTaskLogTitleKey(task.action, task.status))}
            </span>
            <div className="flex items-center gap-1">
              <Button size="sm" variant="ghost" onClick={copyCommand} title={t('agents.card.copyCliCommand')}>
                <Copy className="h-3.5 w-3.5" /> {t('agents.card.copy')}
              </Button>
              {task.status === 'failed' && (
                <Button size="sm" variant="default" onClick={retryAction}>
                  {t('agents.card.retry')}
                </Button>
              )}
              {task.status !== 'running' && (
                <Button size="sm" variant="ghost" onClick={() => setTask(null)}>
                  <X className="h-3.5 w-3.5" /> {t('agents.card.close')}
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
        onConfirmInstall={() => startAgentInstall(selectedChannel)}
        onConfirmOneClick={startOneClickFull}
        shouldIgnoreDismiss={(open) =>
          shouldIgnoreMenuDialogDismiss(ignoreMenuDialogDismissRef.current, open)
        }
      />
    </Card>
  );
}
