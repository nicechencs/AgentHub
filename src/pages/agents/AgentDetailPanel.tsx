import * as React from 'react';
import { InspectSurface } from '@/components/layout/InspectSurface';
import { CopyableFileName } from '@/components/shared/CopyableFileName';
import { OpenDirButton } from '@/components/shared/OpenDirButton';
import { useI18n } from '@/components/shared/LanguageProvider';
import { RouteEndpointTypeText } from '@/components/shared/RouteEndpointUrl';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { useToast } from '@/components/ui/toast';
import { AGENT_MAP } from '@/config/agents';
import {
  InstallFailedError,
  openAgentConfig,
  uninstallAgentDetailed,
} from '@/lib/api/agent';
import { getAgentLivePaths } from '@/lib/api/install';
import { openPathInFileManager } from '@/lib/api/skill';
import { normalizeOpenPath } from '@/lib/path-open';
import type { AgentStatus } from '@/lib/types';
import { AgentCardDialogs, type AgentCardConfirmKind } from './AgentCardDialogs';
import {
  canUninstallProgramInApp,
  formatAgentVersion,
  isSpecialInstallChannel,
  listAgentInstalls,
  spawnInstall,
  uninstallViaLabel,
  type AgentInstall,
} from './agent-card-model';
import {
  agentConversationEndpoints,
  displayAgentConfigDir,
  installChannelKindLabel,
  installLocationSourceLabel,
} from './agent-detail-model';

function Field({ label, value }: { label: string; value?: string | null }) {
  if (!value) return null;
  return (
    <div className="grid grid-cols-[7rem_minmax(0,1fr)] gap-x-3 gap-y-1 text-meta">
      <dt className="text-muted">{label}</dt>
      <dd className="min-w-0 break-all text-secondary">{value}</dd>
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
              <span key={row.id}>
                <RouteEndpointTypeText endpointId={row.id}>{row.path}</RouteEndpointTypeText>
                <span className="text-secondary"> · {t(row.copyKey)}</span>
              </span>
            ))}
          </span>
        )}
      </dd>
    </div>
  );
}

export function AgentDetailPanel({
  agent,
  width,
  onClose,
  onChanged,
}: {
  agent: AgentStatus;
  width: number;
  onClose: () => void;
  onChanged: () => void;
}) {
  const { t } = useI18n();
  const { toast } = useToast();
  const meta = AGENT_MAP[agent.agentId];
  const agentName = meta?.name ?? agent.agentId;
  const installs = listAgentInstalls(agent);
  const spawn = spawnInstall(agent);
  const canUninstallProgram = canUninstallProgramInApp(agent);
  const versionLabel = agent.installed
    ? formatAgentVersion(agent.version)
    : t('agents.card.notInstalled');
  const [confirmDialog, setConfirmDialog] = React.useState<AgentCardConfirmKind>(null);
  const [confirmName, setConfirmName] = React.useState('');
  const [uninstalling, setUninstalling] = React.useState(false);
  const [opening, setOpening] = React.useState<string | null>(null);
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
          description: outcome.message,
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
      toast({ title: t('agents.lifecycle.uninstallFailed'), description: msg, variant: 'danger' });
    } finally {
      setUninstalling(false);
    }
  };

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
        <Field label={t('agents.card.channel')} value={channelLabel} />
        <EndpointTypesField agentId={agent.agentId} />
      </dl>

      <section className="mt-4">
        <h3 className="mb-2 text-body font-medium">{t('agents.detail.installLocations')}</h3>
        {installs.length === 0 ? (
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
                busy={opening === inst.location}
                onOpen={() => openFolder(inst.location)}
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
              disabled={opening !== null || uninstalling}
              title={t('agents.card.openConfigDirTitle')}
              onClick={openConfigDir}
            />
          </div>
        </section>
      ) : null}

      <section className="mt-4">
        <h3 className="mb-2 text-body font-medium">{t('agents.detail.uninstall')}</h3>
        <div className="flex flex-col items-start gap-2">
          {canUninstallProgram ? (
            <Button
              size="sm"
              variant="outline"
              disabled={uninstalling || opening !== null}
              title={t('agents.dialog.uninstallDesc')}
              onClick={() => setConfirmDialog('program')}
            >
              {t('agents.card.uninstallProgram')}
            </Button>
          ) : agent.installed ? (
            <p className="text-meta text-muted">
              {spawn
                ? uninstallViaLabel(spawn.uninstallVia, t)
                : t('agents.card.uninstallViaDesktop')}
            </p>
          ) : null}
          <Button
            size="sm"
            variant="danger"
            disabled={uninstalling || opening !== null}
            title={
              isSpecialInstallChannel(agent.channel)
                ? t('agents.dialog.uninstallConfigKeepsApp')
                : t('agents.dialog.uninstallConfigDesc')
            }
            onClick={() => setConfirmDialog('config')}
          >
            {t('agents.card.uninstallConfig')}
          </Button>
        </div>
      </section>

      <AgentCardDialogs
        agentName={agentName}
        confirmDialog={confirmDialog}
        confirmName={confirmName}
        onConfirmNameChange={setConfirmName}
        uninstalling={uninstalling}
        busy={uninstalling}
        updateState={agent.update?.state}
        onClose={() => {
          setConfirmDialog(null);
          setConfirmName('');
        }}
        onUninstall={(deleteConfig) => void doUninstall(deleteConfig)}
        onConfirmForceUpgrade={() => undefined}
        onConfirmInstall={() => undefined}
        onConfirmOneClick={() => undefined}
        specialInstall={isSpecialInstallChannel(agent.channel)}
      />
    </InspectSurface>
  );
}

function InstallLocationRow({
  agentId,
  inst,
  busy,
  onOpen,
}: {
  agentId: string;
  inst: AgentInstall;
  busy: boolean;
  onOpen: () => void;
}) {
  const { t } = useI18n();
  const versionText = formatAgentVersion(inst.version);
  const openable = Boolean(normalizeOpenPath(inst.location));
  const sourceLabel = installLocationSourceLabel(agentId, inst.source, t);
  return (
    <li className="rounded-card border border-border bg-subtle/60 px-3 py-2">
      <div className="flex items-center justify-between gap-2">
        <div className="flex min-w-0 flex-wrap items-center gap-1.5">
          {inst.spawn ? <Badge>{t('agents.card.spawnCopy')}</Badge> : null}
          {sourceLabel ? (
            <span className="text-meta font-medium text-secondary">{sourceLabel}</span>
          ) : null}
          {versionText ? (
            <span className="font-mono text-meta text-muted">{versionText}</span>
          ) : null}
        </div>
        <OpenDirButton
          labeled
          disabled={!openable || busy}
          title={t('agents.card.openInstallDir')}
          onClick={onOpen}
        />
      </div>
      <div className="mt-1">
        <CopyableFileName path={inst.location} wrap="break" />
      </div>
    </li>
  );
}
