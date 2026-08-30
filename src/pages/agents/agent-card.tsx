import * as React from 'react';
import {
  ArrowUpCircle,
  Copy,
  Eye,
  EyeOff,
  X,
  Zap,
} from 'lucide-react';
import { AgentLogo } from '@/components/shared/AgentLogo';
import { EnvRemediationPanel } from '@/components/shared/EnvRemediationPanel';
import { InlineTerminal } from '@/components/shared/InlineTerminal';
import { LIST_ROW_PAD, ListRow, ListRowBody } from '@/components/shared/ListRow';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import { Tip } from '@/components/ui/tooltip';
import { AGENT_MAP, type InstallChannelMeta } from '@/config/agents';
import { setAgentHidden } from '@/lib/api/agent';
import { openExternalLink } from '@/lib/open-external';
import type { AgentStatus, RuntimeDetect } from '@/lib/types';
import { cn } from '@/lib/utils';
import {
  agentTaskLogTitleKey,
  canInstallAlongsideSpecial,
  formatAgentVersion,
  installRetryButtonVariant,
  isNodeTooOldUpdateNote,
  isSpecialInstallChannel,
  agentListDetailsHint,
  listAgentInstalls,
  programInstalls,
  spawnInstall,
  resolveOfficialSetupUrl,
  uniqueInstallVersions,
} from './agent-card-model';
import { AgentCardDialogs } from './AgentCardDialogs';
import { useAgentCardLifecycle } from './use-agent-card-lifecycle';

export function AgentCard({
  agent,
  runtimes,
  onChanged,
  onEnvChanged,
  onRecheckUpdate,
  sortHandle,
  selected = false,
  onSelect,
}: {
  agent: AgentStatus;
  runtimes: RuntimeDetect[];
  onChanged: () => void;
  onEnvChanged: () => void;
  /** After upgrade, parent may force-refresh update probe for this agent. */
  onRecheckUpdate?: () => void;
  sortHandle?: React.ReactNode;
  selected?: boolean;
  /** Installed agents only — opens the right-hand inspect pane. */
  onSelect?: () => void;
}) {
  const { t } = useI18n();
  const meta = AGENT_MAP[agent.agentId];
  const [selectedChannelId, setSelectedChannelId] = React.useState(() => {
    const detected = agent.channel;
    if (detected && meta?.installChannels.some((c) => c.id === detected)) {
      return detected;
    }
    return meta?.installChannels[0]?.id ?? 'native';
  });

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
    elapsedSec,
    confirmDialog,
    confirmName,
    onConfirmNameChange,
    uninstalling,
    showEnvPanel,
    envAutoStart,
    envCheck,
    canOneClickEnv,
    busy,
    startAgentInstall,
    startOneClickFull,
    startOneClickEnvOnly,
    startUpgrade,
    doUninstall,
    toast,
    openConfirm,
    closeConfirm,
    dismissTask,
    closeEnvironmentPanel,
  } = life;

  const [hiding, setHiding] = React.useState(false);
  const hidden = Boolean(agent.hidden);
  const actionsBusy = busy || hiding;

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
      <Card className={`${LIST_ROW_PAD} text-sm text-muted`}>
        {t('agents.card.unknown', { id: agent.agentId })}
      </Card>
    );
  }

  const selectedChannel: InstallChannelMeta | undefined =
    meta.installChannels.find((c) => c.id === selectedChannelId) ?? meta.installChannels[0];

  if (!selectedChannel) {
    return (
      <Card className={`${LIST_ROW_PAD} text-sm text-muted`}>
        {t('agents.card.channelLoading', { name: meta.name })}
      </Card>
    );
  }

  const updateState = agent.update?.state;
  const checkingUpdate = updateState === 'checking';
  const installs = listAgentInstalls(agent);
  const versions = uniqueInstallVersions(programInstalls(installs));
  const detailsHint = agentListDetailsHint(installs);
  const spawn = spawnInstall(agent);
  const inAppChannel = spawn?.updateVia === 'in_app';
  const upgradable =
    agent.installed &&
    inAppChannel &&
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
  const canForceUpgrade = agent.installed && inAppChannel && !updateUnsupported;
  const showInAppUpgrade =
    agent.installed &&
    (inAppChannel || spawn?.updateVia === 'official' || updateUnsupported);
  const installAlongside = canInstallAlongsideSpecial(agent);
  const latestLabel =
    agent.update?.latestVersion ?? agent.latestVersion ?? undefined;
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
    openConfirm('force-upgrade');
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
      if (isNodeTooOldUpdateNote(agent.update?.note)) return t('agents.card.needsNode22');
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

  return (
    <ListRow
      active={selected}
      indicatorColor={meta.color}
      className={cn(
        LIST_ROW_PAD,
        cardState === 'env_missing' && !hidden && 'border-warning/35',
        hidden && 'opacity-60 grayscale',
      )}
      onOpen={onSelect}
    >
      <ListRowBody
        leading={sortHandle}
        main={(
          <>
            <AgentLogo agentId={agent.agentId} size="sm" />
            <Tip className="truncate text-body font-medium" label={meta.name}>
              {meta.name}
            </Tip>
            {hidden && <Badge>{t('agents.card.hidden')}</Badge>}
            {agent.installed ? (
              versions.map((version) => (
                <span key={version} className="text-meta text-secondary">
                  {version}
                </span>
              ))
            ) : (
              <span className="text-meta text-muted">{t('agents.card.notInstalled')}</span>
            )}
            {detailsHint ? (
              <span className="text-meta text-muted">
                {t(detailsHint.key, detailsHint.params)}
              </span>
            ) : null}
          </>
        )}
        actions={hidden ? (
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
            {installAlongside ? (
              <Button
                size="sm"
                variant={installRetryButtonVariant(task?.status)}
                onClick={() =>
                  installFailed ? retryAction() : openConfirm('install')
                }
                disabled={busy}
                title={
                  installFailed
                    ? t('agents.card.retry')
                    : t('agents.card.installWithChannel', { id: selectedChannel.id })
                }
              >
                <Zap className="h-3.5 w-3.5" />
                {installFailed ? t('agents.card.retry') : t('agents.card.install')}
              </Button>
            ) : null}
            {showInAppUpgrade ? (
              <Button
                size="icon"
                variant="secondary"
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
                    upgradable && 'text-success',
                    checkingUpdate && 'animate-pulse opacity-70',
                  )}
                />
              </Button>
            ) : null}
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
          </>
        ) : cardState === 'env_missing' ? (
          <>
            <Button
              size="sm"
              variant="secondary"
              onClick={canOneClickEnv ? () => openConfirm('oneclick') : startOneClickEnvOnly}
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
          </>
        ) : (
          <>
            <Button
              size="sm"
              variant={installRetryButtonVariant(task?.status)}
              onClick={() => (installFailed ? retryAction() : openConfirm('install'))}
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
          </>
        )}
      />

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
            onDismiss={closeEnvironmentPanel}
            onDone={() => {
              closeEnvironmentPanel();
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
                <Button size="sm" variant="ghost" onClick={dismissTask}>
                  <X className="h-3.5 w-3.5" /> {t('agents.card.close')}
                </Button>
              )}
            </div>
          </div>
          {task.diagnosis ? (
            <p className="mb-1 text-xs text-secondary">{task.diagnosis}</p>
          ) : null}
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
        onConfirmNameChange={onConfirmNameChange}
        uninstalling={uninstalling}
        busy={busy}
        updateState={updateState}
        onClose={closeConfirm}
        onUninstall={(deleteConfig) => void doUninstall(deleteConfig)}
        onConfirmForceUpgrade={startUpgrade}
        onConfirmInstall={() => startAgentInstall(selectedChannel)}
        onConfirmOneClick={startOneClickFull}
        specialInstall={isSpecialInstallChannel(agent.channel)}
      />
    </ListRow>
  );
}
