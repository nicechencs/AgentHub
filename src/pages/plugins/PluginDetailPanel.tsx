import { useState } from 'react';
import { InspectSurface } from '@/components/layout/InspectSurface';
import { CopyableFileName } from '@/components/shared/CopyableFileName';
import { OpenDirButton } from '@/components/shared/OpenDirButton';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Badge } from '@/components/ui/badge';
import { Switch } from '@/components/ui/switch';
import { agentDisplayName } from '@/config/agents';
import type { PluginComponent, PluginEntry } from '@/lib/backend/contracts/plugin-types';
import type { TranslateFn } from '@/lib/i18n';
import { canToggleListedPlugin } from './can-toggle';
import { pluginVersionView } from './plugin-version-model';

function kindLabel(kind: string, t: TranslateFn): string {
  switch (kind) {
    case 'skills':
      return t('plugins.detail.kindSkills');
    case 'commands':
      return t('plugins.detail.kindCommands');
    case 'agents':
      return t('plugins.detail.kindAgents');
    case 'hooks':
      return t('plugins.detail.kindHooks');
    case 'mcp':
      return t('plugins.detail.kindMcp');
    case 'lsp':
      return t('plugins.detail.kindLsp');
    default:
      return kind;
  }
}

function scopeLabel(scope: string, t: TranslateFn): string {
  switch (scope) {
    case 'user':
      return t('plugins.detail.scopeUser');
    case 'project':
      return t('plugins.detail.scopeProject');
    case 'local':
      return t('plugins.detail.scopeLocal');
    case 'managed':
      return t('plugins.detail.scopeManaged');
    default:
      return scope;
  }
}

function Field({ label, value }: { label: string; value?: string | null }) {
  if (!value) return null;
  return (
    <div className="grid grid-cols-[7rem_minmax(0,1fr)] gap-x-3 gap-y-1 text-meta">
      <dt className="text-muted">{label}</dt>
      <dd className="min-w-0 break-all text-secondary">{value}</dd>
    </div>
  );
}

function groupComponents(components: PluginComponent[]): Array<[string, PluginComponent[]]> {
  const order = ['skills', 'commands', 'agents', 'hooks', 'mcp', 'lsp'];
  const map = new Map<string, PluginComponent[]>();
  for (const item of components) {
    const list = map.get(item.kind) ?? [];
    list.push(item);
    map.set(item.kind, list);
  }
  const keys = [...order.filter((k) => map.has(k)), ...[...map.keys()].filter((k) => !order.includes(k))];
  return keys.map((k) => [k, map.get(k) ?? []]);
}

export function PluginDetailPanel({
  plugin,
  width,
  onClose,
  onLocate,
  onToggle,
}: {
  plugin: PluginEntry;
  width: number;
  onClose: () => void;
  onLocate: (path: string) => void;
  onToggle?: (plugin: PluginEntry, enabled: boolean) => Promise<void>;
}) {
  const { t } = useI18n();
  const [busy, setBusy] = useState<'enable' | 'disable' | null>(null);
  const canToggle = canToggleListedPlugin(plugin.agent) && Boolean(onToggle);
  const enabled = plugin.enabled === true;
  const description = plugin.description?.trim() || undefined;
  const version = pluginVersionView(plugin);
  const versionValue =
    version.listBadge === 'notInstalled'
      ? t('plugins.list.notInstalled')
      : version.installed;
  const showRequested =
    Boolean(version.requested) &&
    (version.kind === 'pinned' ||
      version.kind === 'mismatch' ||
      version.kind === 'missing' ||
      version.kind === 'git');

  async function toggle(next: boolean) {
    if (!onToggle || busy || next === enabled) return;
    setBusy(next ? 'enable' : 'disable');
    try {
      await onToggle(plugin, next);
    } finally {
      setBusy(null);
    }
  }

  const actions = canToggle ? (
    <div className="flex items-center gap-2">
      <span className="text-meta text-secondary">
        {busy === 'enable'
          ? t('plugins.actions.enabling')
          : busy === 'disable'
            ? t('plugins.actions.disabling')
            : enabled
              ? t('plugins.actions.enabled')
              : t('plugins.actions.disabled')}
      </span>
      <Switch
        checked={enabled}
        disabled={busy !== null}
        aria-label={t('plugins.actions.toggle')}
        onCheckedChange={(next) => void toggle(next)}
      />
    </div>
  ) : undefined;

  return (
    <InspectSurface
      asPanel
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
      title={plugin.name}
      description={description}
      showCancel={false}
      primary={actions}
      width={width}
    >
      {canToggle ? (
        <p className="mb-3 text-meta text-muted">{t('plugins.actions.disableHint')}</p>
      ) : null}

      <section>
        <h3 className="mb-2 text-body font-medium">{t('plugins.detail.components')}</h3>
        {plugin.components.length === 0 ? (
          <p className="text-meta text-muted">{t('plugins.detail.noComponents')}</p>
        ) : (
          <div className="flex flex-col gap-3">
            {groupComponents(plugin.components).map(([kind, items]) => (
              <div key={kind}>
                <p className="mb-1 text-meta font-medium text-secondary">{kindLabel(kind, t)}</p>
                <ul className="flex flex-col gap-1">
                  {items.map((item) => (
                    <li key={`${kind}:${item.name}`} className="text-meta text-secondary">
                      <span className="font-medium text-primary">{item.name}</span>
                      {item.description ? (
                        <span className="text-muted"> — {item.description}</span>
                      ) : null}
                    </li>
                  ))}
                </ul>
              </div>
            ))}
          </div>
        )}
      </section>

      <dl className="mt-4 flex flex-col gap-2">
        <Field label={t('plugins.detail.agent')} value={agentDisplayName(plugin.agent)} />
        <Field label={t('plugins.detail.version')} value={versionValue} />
        {showRequested ? (
          <Field label={t('plugins.detail.requestedVersion')} value={version.requested} />
        ) : null}
        <Field label={t('plugins.detail.marketplace')} value={plugin.marketplace} />
        <Field
          label={t('plugins.detail.scope')}
          value={plugin.scope ? scopeLabel(plugin.scope, t) : null}
        />
        {plugin.trusted === false ? (
          <div className="grid grid-cols-[7rem_minmax(0,1fr)] gap-x-3 gap-y-1 text-meta">
            <dt className="text-muted">{t('plugins.detail.trusted')}</dt>
            <dd>
              <Badge variant="warning">{t('plugins.list.untrusted')}</Badge>
            </dd>
          </div>
        ) : null}
        {plugin.path ? (
          <div className="grid grid-cols-[7rem_minmax(0,1fr)] gap-x-3 gap-y-1 text-meta">
            <dt className="text-muted">{t('plugins.detail.path')}</dt>
            <dd className="min-w-0">
              <CopyableFileName path={plugin.path} wrap="break" />
              <OpenDirButton
                labeled
                className="mt-1"
                title={plugin.path}
                onClick={() => onLocate(plugin.path!)}
              />
            </dd>
          </div>
        ) : null}
      </dl>
      {version.hintKey ? (
        <p className="mt-3 text-meta text-muted">{t(version.hintKey)}</p>
      ) : null}
    </InspectSurface>
  );
}
