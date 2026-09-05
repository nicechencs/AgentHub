import * as React from 'react';
import { ArrowUpCircle, Copy, Trash2 } from 'lucide-react';
import { InspectSurface } from '@/components/layout/InspectSurface';
import { CopyableFileName } from '@/components/shared/CopyableFileName';
import { OpenDirButton } from '@/components/shared/OpenDirButton';
import { useI18n } from '@/components/shared/LanguageProvider';
import { RouteEndpointTypeText } from '@/components/shared/RouteEndpointUrl';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Hint } from '@/components/ui/tooltip';
import { useToast } from '@/components/ui/toast';
import { AGENT_MAP, type InstallChannelMeta } from '@/config/agents';
import {
  InstallFailedError,
  installAgentDetailed,
  openAgentConfig,
  uninstallAgentDetailed,
  upgradeAgentDetailed,
} from '@/lib/api/agent';
import { getAgentLivePaths } from '@/lib/api/install';
import { checkChannelEnv, formatMissingList } from '@/lib/env';
import { openPathInFileManager } from '@/lib/api/skill';
import { openExternalLink } from '@/lib/open-external';
import { normalizeOpenPath } from '@/lib/path-open';
import type { AgentStatus, RuntimeDetect } from '@/lib/types';
import { cn } from '@/lib/utils';
import { AgentCardDialogs, type AgentCardConfirmKind } from './AgentCardDialogs';
import { AgentInstallButton } from './AgentInstallButton';
import {
  agentLinuxInstallUnsupported,
  agentUninstallControl,
  agentUpgradeControl,
  agentUpgradeHint,
  extraCopyUpdateHint,
  formatAgentVersion,
  isLeftoverInstallSource,
  isSpecialInstallChannel,
  listAgentInstalls,
  resolveOfficialSetupUrl,
  spawnInstall,
  uninstallViaLabel,
  type AgentInstall,
  type AgentUninstallControl,
  type AgentUpgradeControl,
} from './agent-card-model';
import {
  agentConversationEndpoints,
  copyableChannelCommand,
  displayAgentConfigDir,
  installChannelKindLabel,
  installLocationSourceLabel,
  missingCatalogChannels,
  missingChannelStatusKey,
} from './agent-detail-model';
import { localizeInstallCopy } from './install-labels';

function CopyableChannelName({
  label,
  command,
  className,
}: {
  label: string;
  command?: string;
  className?: string;
}) {
  const { t } = useI18n();
  const { toast } = useToast();
  if (!command) return <span className={className}>{label}</span>;
  return (
    <Hint label={t('agents.card.copyCommand')}>
      <button
        type="button"
        className={cn(className, 'hover:text-accent')}
        aria-label={t('agents.card.copyCommand')}
        onClick={() => {
          void navigator.clipboard.writeText(command).then(() => {
            toast({ title: t('agents.env.commandCopied'), variant: 'success' });
          }).catch(() => {});
        }}
      >
        {label}
      </button>
    </Hint>
  );
}

function Field({
  label,
  value,
  copyText,
}: {
  label: string;
  value?: string | null;
  copyText?: string;
}) {
  if (!value) return null;
  return (
    <div className="grid grid-cols-[7rem_minmax(0,1fr)] gap-x-3 gap-y-1 text-meta">
      <dt className="text-muted">{label}</dt>
      <dd className="min-w-0 break-all text-secondary">
        <CopyableChannelName label={value} command={copyText} />
      </dd>
    </div>
  );
}

function EndpointTypesField({ agentId }: { agentId: string }) {
  const { t } = useI18n();
  const rows = agentConversationEndpoints(agentId);
  return (
    <div className="grid grid-cols-[7rem_minmax(0,1fr)] gap-x-3 gap-y-1 text-meta">
      <dt className="text-muted">{t('agents.detail.endpointTypes')}</dt>
      <dd className="min-w-0 break-all">
        {rows.length === 0 ? (
          <span className="text-secondary">{t('agents.detail.endpointDependsOnLogin')}</span>
        ) : (
          <span className="flex flex-col gap-0.5 font-mono">
            {rows.map((row) => (
              <RouteEndpointTypeText key={row.id} endpointId={row.id} brandAgentId={row.brandAgentId}>
                {row.path}
              </RouteEndpointTypeText>
            ))}
          </span>
        )}
      </dd>
    </div>
  );
}

export function AgentDetailPanel({
  agent,
  runtimes = [],
  width,
  onClose,
  onChanged,
}: {
  agent: AgentStatus;
  runtimes?: RuntimeDetect[];
  width: number;
  onClose: () => void;
  onChanged: () => void;
}) {
  const { t } = useI18n();
  const { toast } = useToast();
  const meta = AGENT_MAP[agent.agentId];
  const agentName = meta?.name ?? agent.agentId;
  const installs = listAgentInstalls(agent);
  const missingChannels = missingCatalogChannels(agent);
  const spawn = spawnInstall(agent);
  const versionLabel = agent.installed
    ? formatAgentVersion(agent.version)
    : t('agents.card.notInstalled');
  const [confirmDialog, setConfirmDialog] = React.useState<AgentCardConfirmKind>(null);
  const [confirmName, setConfirmName] = React.useState('');
  const [pendingChannel, setPendingChannel] = React.useState<InstallChannelMeta | null>(null);
  const [pendingUpgrade, setPendingUpgrade] = React.useState<'spawn' | InstallChannelMeta | null>(null);
  const [uninstalling, setUninstalling] = React.useState(false);
  const [installing, setInstalling] = React.useState(false);
  const [upgrading, setUpgrading] = React.useState(false);
  const [opening, setOpening] = React.useState<string | null>(null);
  const officialSetupUrl = resolveOfficialSetupUrl(
    agent.update?.setupUrl,
    meta?.installChannels ?? [],
  );
  const linuxUnsupported = agentLinuxInstallUnsupported(agent.agentId);
  const latestVersion = agent.update?.latestVersion ?? agent.latestVersion;
  const checkingUpdate = agent.update?.state === 'checking';
  const rowBusy = installing || uninstalling || upgrading || opening !== null;
  const [resolvedConfigDir, setResolvedConfigDir] = React.useState<string | null>(null);
  const channelLabel = installChannelKindLabel(
    agent.agentId,
    spawn?.source ?? agent.channel,
    t,
  );
  const configDir = displayAgentConfigDir(agent.agentId, resolvedConfigDir);

  React.useEffect(() => {
    let cancelled = false;
    setResolvedConfigDir(null);
    void getAgentLivePaths(agent.agentId)
      .then((paths) => {
        if (!cancelled) setResolvedConfigDir(paths.openDir);
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [agent.agentId]);

  const openFolder = (location: string) => {
    const target = normalizeOpenPath(location);
    if (!target) {
      toast({ title: t('agents.env.noInstallPath'), variant: 'danger' });
      return;
    }
    void (async () => {
      setOpening(location);
      try {
        const path = await openPathInFileManager(target);
        toast({
          title: t('agents.env.openedInstallDir'),
          description: path,
          variant: 'success',
        });
      } catch (e) {
        toast({
          title: t('agents.env.openFailed'),
          description: e instanceof Error ? e.message : String(e),
          variant: 'danger',
        });
      } finally {
        setOpening(null);
      }
    })();
  };

  const openConfigDir = () => {
    void (async () => {
      setOpening('config');
      try {
        const path = await openAgentConfig(agent.agentId);
        toast({
          title: t('agents.env.openedConfigDir'),
          description: path ?? agentName,
          variant: 'success',
        });
      } catch (e) {
        toast({
          title: t('agents.env.openFailed'),
          description: e instanceof Error ? e.message : String(e),
          variant: 'danger',
        });
      } finally {
        setOpening(null);
      }
    })();
  };

  const doUninstall = async (deleteConfig: boolean) => {
    setUninstalling(true);
    try {
      const outcome = await uninstallAgentDetailed(agent.agentId, deleteConfig);
      if (!outcome.ok) {
        toast({
          title: t('agents.lifecycle.uninstallIncomplete'),
          description: localizeInstallCopy(outcome.message, t),
          variant: 'danger',
        });
        return;
      }
      setConfirmDialog(null);
      setConfirmName('');
      onChanged();
      toast({
        title: t('agents.lifecycle.uninstalled', { name: agentName }),
        description: deleteConfig ? t('agents.lifecycle.configHandled') : undefined,
        variant: 'success',
      });
    } catch (e) {
      const msg = e instanceof InstallFailedError ? e.message : String(e);
      toast({
        title: t('agents.lifecycle.uninstallFailed'),
        description: localizeInstallCopy(msg, t),
        variant: 'danger',
      });
    } finally {
      setUninstalling(false);
    }
  };

  const doInstall = async (channel: InstallChannelMeta) => {
    if (linuxUnsupported) {
      toast({
        title: t('agents.card.linuxUnsupported'),
        description: t('agents.card.linuxUnsupportedHint'),
        variant: 'danger',
      });
      return;
    }
    const check = checkChannelEnv(channel, runtimes);
    if (!check.ready) {
      toast({
        title: t('agents.lifecycle.envNotReady'),
        description: t('agents.lifecycle.handleFirst', {
          list: formatMissingList([...check.missing, ...check.outdated, ...check.broken]),
        }),
        variant: 'danger',
      });
      return;
    }
    setInstalling(true);
    try {
      const outcome = await installAgentDetailed(agent.agentId, channel.id, { installDeps: false });
      if (!outcome.ok) {
        toast({
          title: t('agents.lifecycle.installFailed'),
          description: localizeInstallCopy(outcome.message, t),
          variant: 'danger',
        });
        return;
      }
      setPendingChannel(null);
      onChanged();
      toast({
        title: t('agents.lifecycle.installDone', { name: agentName }),
        variant: 'success',
      });
    } catch (e) {
      const msg = e instanceof InstallFailedError ? e.message : String(e);
      toast({
        title: t('agents.lifecycle.installFailed'),
        description: localizeInstallCopy(msg, t),
        variant: 'danger',
      });
    } finally {
      setInstalling(false);
    }
  };

  const openOfficialSetup = () => {
    if (linuxUnsupported) {
      toast({
        title: t('agents.card.linuxUnsupported'),
        description: t('agents.card.linuxUnsupportedHint'),
        variant: 'danger',
      });
      return;
    }
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

  const doUpgrade = async (target: 'spawn' | InstallChannelMeta) => {
    setUpgrading(true);
    try {
      const outcome =
        target === 'spawn'
          ? await upgradeAgentDetailed(agent.agentId)
          : await installAgentDetailed(agent.agentId, target.id, { installDeps: false });
      if (!outcome.ok) {
        toast({
          title: t('agents.lifecycle.upgradeFailed'),
          description: localizeInstallCopy(outcome.message, t),
          variant: 'danger',
        });
        return;
      }
      setPendingUpgrade(null);
      onChanged();
      toast({
        title: t('agents.lifecycle.upgraded', { name: agentName }),
        variant: 'success',
      });
    } catch (e) {
      const msg = e instanceof InstallFailedError ? e.message : String(e);
      toast({
        title: t('agents.lifecycle.upgradeFailed'),
        description: localizeInstallCopy(msg, t),
        variant: 'danger',
      });
    } finally {
      setUpgrading(false);
    }
  };

  const catalogChannel = (source: string) =>
    meta?.installChannels.find((channel) => channel.id === source);

  return (
    <InspectSurface
      asPanel
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
      title={agentName}
      description={versionLabel}
      showCancel={false}
      width={width}
    >
      <dl className="flex flex-col gap-2">
        <Field
          label={t('agents.card.channel')}
          value={channelLabel}
          copyText={copyableChannelCommand(agent.agentId, spawn?.source ?? agent.channel, t)}
        />
        <EndpointTypesField agentId={agent.agentId} />
      </dl>

      <section className="mt-4">
        <h3 className="mb-2 text-body font-medium">{t('agents.detail.installLocations')}</h3>
        {installs.length === 0 && missingChannels.length === 0 ? (
          <p className="text-meta text-muted">
            {agent.installed ? t('agents.env.noInstallPath') : t('agents.card.notInstalled')}
          </p>
        ) : (
          <ul className="flex flex-col gap-3">
            {installs.map((inst) => (
              <InstallLocationRow
                key={`${inst.source}:${inst.location}`}
                agentId={agent.agentId}
                inst={inst}
                latestVersion={latestVersion}
                updateState={inst.spawn ? agent.update?.state : undefined}
                setupUrl={officialSetupUrl}
                note={localizeInstallCopy(agent.update?.note ?? '', t)}
                checking={checkingUpdate}
                busy={rowBusy}
                opening={opening === inst.location}
                onOpen={() => openFolder(inst.location)}
                onUninstall={() => setConfirmDialog('program')}
                onUpgrade={() => {
                  const upgradable =
                    extraCopyUpdateHint(inst.source, inst.version, latestVersion) === 'update_available'
                    || (inst.spawn && agent.update?.state === 'update_available');
                  if (inst.spawn) {
                    if (upgradable) {
                      void doUpgrade('spawn');
                      return;
                    }
                    setPendingUpgrade('spawn');
                    setConfirmDialog('force-upgrade');
                    return;
                  }
                  const channel = catalogChannel(inst.source);
                  if (!channel) return;
                  if (upgradable) {
                    void doUpgrade(channel);
                    return;
                  }
                  setPendingUpgrade(channel);
                  setConfirmDialog('force-upgrade');
                }}
                onOpenSetup={openOfficialSetup}
              />
            ))}
            {missingChannels.map((channel) => (
              <MissingChannelRow
                key={`missing:${channel.id}`}
                agentId={agent.agentId}
                channel={channel}
                agentInstalled={agent.installed || installs.length > 0}
                busy={rowBusy}
                onInstall={() => {
                  setPendingChannel(channel);
                  setConfirmDialog('install');
                }}
              />
            ))}
          </ul>
        )}
      </section>

      {configDir ? (
        <section className="mt-4">
          <h3 className="mb-2 text-body font-medium">{t('agents.detail.configDir')}</h3>
          <div className="flex items-center justify-between gap-2 rounded-card border border-border bg-subtle/60 px-3 py-2">
            <CopyableFileName path={configDir} wrap="break" className="min-w-0 flex-1" />
            <OpenDirButton
              labeled
              disabled={rowBusy}
              title={t('agents.card.openConfigDirTitle')}
              onClick={openConfigDir}
            />
          </div>
        </section>
      ) : null}

      <section className="mt-4">
        <h3 className="mb-2 text-body font-medium">{t('agents.detail.uninstall')}</h3>
        <Button
          size="sm"
          variant="dangerOutline"
          disabled={rowBusy}
          title={
            isSpecialInstallChannel(agent.channel)
              ? t('agents.dialog.uninstallConfigKeepsApp')
              : t('agents.dialog.uninstallConfigDesc')
          }
          onClick={() => setConfirmDialog('config')}
        >
          {t('agents.card.uninstallConfig')}
        </Button>
      </section>

      <AgentCardDialogs
        agentName={agentName}
        confirmDialog={confirmDialog}
        confirmName={confirmName}
        onConfirmNameChange={setConfirmName}
        uninstalling={uninstalling}
        busy={rowBusy}
        updateState={agent.update?.state}
        onClose={() => {
          setConfirmDialog(null);
          setConfirmName('');
        }}
        onUninstall={(deleteConfig) => void doUninstall(deleteConfig)}
        onConfirmForceUpgrade={() => {
          if (pendingUpgrade) void doUpgrade(pendingUpgrade);
        }}
        onConfirmInstall={() => {
          if (pendingChannel) void doInstall(pendingChannel);
        }}
        onConfirmOneClick={() => undefined}
        specialInstall={isSpecialInstallChannel(agent.channel)}
      />
    </InspectSurface>
  );
}

function InstallLocationRow({
  agentId,
  inst,
  latestVersion,
  updateState,
  setupUrl,
  note,
  checking,
  busy,
  opening,
  onOpen,
  onUninstall,
  onUpgrade,
  onOpenSetup,
}: {
  agentId: string;
  inst: AgentInstall;
  latestVersion?: string;
  updateState?: string;
  setupUrl?: string;
  note?: string;
  checking: boolean;
  busy: boolean;
  opening: boolean;
  onOpen: () => void;
  onUninstall: () => void;
  onUpgrade: () => void;
  onOpenSetup: () => void;
}) {
  const { t } = useI18n();
  const versionText = formatAgentVersion(inst.version);
  const openable = Boolean(normalizeOpenPath(inst.location));
  const sourceLabel = installLocationSourceLabel(agentId, inst.source, t);
  const linuxUnsupported = agentLinuxInstallUnsupported(agentId);
  const upgradeControl = agentUpgradeControl({
    installed: true,
    updateVia: inst.updateVia,
    updateState,
    setupUrl,
    linuxUnsupported,
  });
  const upgradable =
    upgradeControl.kind === 'in_app'
    && (
      extraCopyUpdateHint(inst.source, inst.version, latestVersion) === 'update_available'
      || updateState === 'update_available'
    );
  const upgradeTooltip = upgradeControl.muted
    ? agentUpgradeHint(upgradeControl, { updateVia: inst.updateVia, note, linuxUnsupported, t })
    : upgradable
      ? t('agents.update.available')
      : t('agents.update.forceLatest');
  const leftover = isLeftoverInstallSource(inst.source);
  const uninstallControl = agentUninstallControl(inst.uninstallVia);
  const uninstallTooltip = uninstallControl.muted || leftover
    ? uninstallViaLabel(inst.uninstallVia, t)
    : t('agents.dialog.uninstallDesc');
  return (
    <li
      className={cn(
        'rounded-card border px-3 py-2',
        leftover
          ? 'border-warning/45 bg-warning/5'
          : 'border-border bg-subtle/60',
      )}
    >
      <div className="flex items-center justify-between gap-2">
        <div className="flex min-w-0 flex-wrap items-center gap-1.5">
          {inst.spawn ? <Badge variant="success">{t('agents.card.spawnCopy')}</Badge> : null}
          {leftover ? (
            <Badge variant="warning">{t('agents.card.leftoverDoNotLaunch')}</Badge>
          ) : null}
          {sourceLabel ? (
            <CopyableChannelName
              label={sourceLabel}
              command={copyableChannelCommand(agentId, inst.source, t)}
              className={cn(
                'text-meta font-medium',
                leftover ? 'text-warning' : 'text-secondary',
              )}
            />
          ) : null}
          {versionText ? (
            <span
              className={cn(
                'font-mono text-meta',
                leftover ? 'text-warning' : 'text-muted',
              )}
            >
              {versionText}
            </span>
          ) : null}
        </div>
        <div className="flex shrink-0 items-center gap-1">
          <ChannelUpgradeButton
            control={upgradeControl}
            upgradable={upgradable}
            checking={checking}
            busy={busy}
            tooltip={upgradeTooltip}
            onUpgrade={onUpgrade}
            onOpenSetup={onOpenSetup}
          />
          <OpenDirButton
            disabled={!openable || opening || busy}
            title={t('agents.card.openInstallDir')}
            onClick={onOpen}
          />
          <ChannelUninstallButton
            control={uninstallControl}
            busy={busy}
            tooltip={uninstallTooltip}
            ariaLabel={
              leftover
                ? t('agents.card.viaLeftover')
                : t('agents.card.uninstallProgram')
            }
            onUninstall={onUninstall}
          />
        </div>
      </div>
      <div className="mt-1">
        <CopyableFileName path={inst.location} wrap="break" />
      </div>
      {leftover ? (
        <p className="mt-1 text-meta text-warning">{t('agents.card.leftoverDoNotLaunch')}</p>
      ) : null}
    </li>
  );
}

function ChannelUpgradeButton({
  control,
  upgradable,
  checking,
  busy,
  tooltip,
  onUpgrade,
  onOpenSetup,
}: {
  control: AgentUpgradeControl;
  upgradable: boolean;
  checking: boolean;
  busy: boolean;
  tooltip: string;
  onUpgrade: () => void;
  onOpenSetup: () => void;
}) {
  const { t } = useI18n();
  if (!control.show) return null;
  return (
    <Button
        size="icon"
        variant={control.muted ? 'outline' : 'secondary'}
        className={control.muted ? 'text-muted' : undefined}
        disabled={busy || checking || control.kind === 'hint_only'}
        title={tooltip}
        aria-label={
          control.kind === 'open_setup'
            ? t('agents.card.openOfficialUpdate')
            : control.muted
              ? tooltip || t('agents.card.unsupportedUpdate')
              : upgradable
                ? t('agents.card.update')
                : t('agents.card.forceUpgrade')
        }
        onClick={
          control.kind === 'open_setup'
            ? onOpenSetup
            : control.kind === 'in_app'
              ? onUpgrade
              : undefined
        }
      >
        <ArrowUpCircle
          className={cn(
            'h-3.5 w-3.5',
            !control.muted && upgradable && 'text-success',
            control.muted && 'text-muted',
            checking && 'animate-pulse opacity-70',
          )}
        />
      </Button>
  );
}

function ChannelUninstallButton({
  control,
  busy,
  tooltip,
  ariaLabel,
  onUninstall,
}: {
  control: AgentUninstallControl;
  busy: boolean;
  tooltip: string;
  ariaLabel?: string;
  onUninstall: () => void;
}) {
  const { t } = useI18n();
  if (!control.show) return null;
  return (
    <Button
        size="icon"
        variant="outline"
        className={control.muted ? 'text-muted' : 'text-danger hover:text-danger'}
        disabled={busy || control.muted}
        title={tooltip}
        aria-label={ariaLabel ?? t('agents.card.uninstallProgram')}
        onClick={control.muted ? undefined : onUninstall}
      >
        <Trash2 className={cn('h-3.5 w-3.5', control.muted && 'text-muted')} />
      </Button>
  );
}

function CopyableCommand({ command }: { command: string }) {
  const { t } = useI18n();
  const { toast } = useToast();
  const copy = () => {
    void navigator.clipboard.writeText(command).then(() => {
      toast({ title: t('agents.env.commandCopied'), variant: 'success' });
    }).catch(() => {});
  };
  return (
    <Hint label={t('agents.card.copyCommand')}>
      <button
        type="button"
        className="mt-1 flex w-full min-w-0 items-start gap-1.5 text-left text-meta text-muted hover:text-accent"
        aria-label={t('agents.card.copyCommand')}
        onClick={copy}
      >
        <span className="min-w-0 flex-1 break-all font-mono">{command}</span>
        <Copy className="mt-0.5 h-3 w-3 shrink-0" />
      </button>
    </Hint>
  );
}

function MissingChannelRow({
  agentId,
  channel,
  agentInstalled,
  busy,
  onInstall,
}: {
  agentId: string;
  channel: InstallChannelMeta;
  /** True when another channel already satisfies this agent (alternate = optional). */
  agentInstalled: boolean;
  busy: boolean;
  onInstall: () => void;
}) {
  const { t } = useI18n();
  const linuxUnsupported = agentLinuxInstallUnsupported(agentId);
  const statusKey = missingChannelStatusKey({ agentInstalled, linuxUnsupported });
  const sourceLabel = installLocationSourceLabel(agentId, channel.id, t);
  const command = channel.command.trim();
  const nameCommand = copyableChannelCommand(agentId, channel.id, t);
  return (
    <li className="rounded-card border border-border bg-subtle/60 px-3 py-2">
      <div className="flex items-center justify-between gap-2">
        <div className="flex min-w-0 flex-wrap items-center gap-1.5">
          {sourceLabel ? (
            <CopyableChannelName
              label={sourceLabel}
              command={nameCommand}
              className="text-meta font-medium text-secondary"
            />
          ) : null}
          <span className="text-meta text-muted">{t(statusKey)}</span>
        </div>
        <AgentInstallButton iconOnly busy={busy} channelId={channel.id} linuxUnsupported={linuxUnsupported} onClick={onInstall} />
      </div>
      {command ? (
        nameCommand ? (
          <p className="mt-1 break-all font-mono text-meta text-muted">{command}</p>
        ) : (
          <CopyableCommand command={command} />
        )
      ) : null}
    </li>
  );
}
