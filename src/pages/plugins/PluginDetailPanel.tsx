import { FolderOpen } from 'lucide-react';
import { InspectSurface } from '@/components/layout/InspectSurface';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
import { agentDisplayName } from '@/config/agents';
import type { PluginComponent, PluginEntry } from '@/lib/backend/contracts/plugin-types';
import type { TranslateFn } from '@/lib/i18n';

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

function triState(value: boolean | null | undefined, t: TranslateFn): string {
  if (value === true) return t('plugins.detail.yes');
  if (value === false) return t('plugins.detail.no');
  return t('plugins.detail.unknown');
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
}: {
  plugin: PluginEntry;
  width: number;
  onClose: () => void;
  onLocate: (path: string) => void;
}) {
  const { t } = useI18n();
  const sourceLabel =
    plugin.source === 'cli'
      ? t('plugins.detail.sourceCli')
      : plugin.source === 'live'
        ? t('plugins.detail.sourceLive')
        : plugin.source;

  return (
    <InspectSurface
      asPanel
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
      title={plugin.name}
      description={plugin.version ?? undefined}
      showCancel={false}
      width={width}
    >
      <dl className="flex flex-col gap-2">
        <Field label={t('plugins.detail.name')} value={plugin.name} />
        <Field label={t('plugins.detail.agent')} value={agentDisplayName(plugin.agent)} />
        <Field label={t('plugins.detail.marketplace')} value={plugin.marketplace} />
        <Field label={t('plugins.detail.version')} value={plugin.version} />
        <Field label={t('plugins.detail.scope')} value={plugin.scope} />
        <Field label={t('plugins.detail.enabled')} value={triState(plugin.enabled, t)} />
        <Field label={t('plugins.detail.trusted')} value={triState(plugin.trusted, t)} />
        <Field label={t('plugins.detail.source')} value={sourceLabel} />
        {plugin.path ? (
          <div className="grid grid-cols-[7rem_minmax(0,1fr)] gap-x-3 gap-y-1 text-meta">
            <dt className="text-muted">{t('plugins.detail.path')}</dt>
            <dd className="min-w-0">
              <p className="break-all font-mono text-secondary">{plugin.path}</p>
              <Button
                size="sm"
                variant="ghost"
                className="mt-1 h-7 px-2"
                onClick={() => onLocate(plugin.path!)}
              >
                <FolderOpen className="h-3 w-3" />
                {t('plugins.detail.directory')}
              </Button>
            </dd>
          </div>
        ) : null}
      </dl>

      <section className="mt-4">
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
    </InspectSurface>
  );
}
