import { AgentDot } from '@/components/shared/AgentDot';
import { ListRow } from '@/components/shared/ListRow';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Badge } from '@/components/ui/badge';
import { Tip } from '@/components/ui/tooltip';
import { AGENT_MAP } from '@/config/agents';
import type { PluginEntry } from '@/lib/backend/contracts/plugin-types';
import { pluginVersionView } from './plugin-version-model';

function packDescription(plugin: PluginEntry): string | null {
  const description = plugin.description?.trim() ?? '';
  if (!description || description === plugin.name) return null;
  return description;
}

export function PluginPackList({
  plugins,
  showAgent,
  activeId,
  onOpen,
}: {
  plugins: PluginEntry[];
  showAgent: boolean;
  activeId: string | null;
  onOpen: (plugin: PluginEntry) => void;
}) {
  const { t } = useI18n();
  return (
    <div className="flex flex-col gap-2" data-help="plugins-list">
      {plugins.map((plugin) => {
        const active = plugin.id === activeId;
        const meta = AGENT_MAP[plugin.agent];
        const description = packDescription(plugin);
        const version = pluginVersionView(plugin);
        return (
          <ListRow
            key={plugin.id}
            role="button"
            active={active}
            indicatorColor={meta?.color}
            className="p-3"
            onOpen={() => onOpen(plugin)}
          >
            <div className="flex min-w-0 items-start gap-2">
              {showAgent ? <AgentDot agentId={plugin.agent} className="mt-0.5" /> : null}
              <div className="min-w-0 flex-1">
                <div className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1">
                  <Tip className="truncate text-body font-medium" label={plugin.name}>
                    {plugin.name}
                  </Tip>
                  {version.versionLabel ? (
                    <span className="text-meta text-muted">{version.versionLabel}</span>
                  ) : null}
                  {plugin.enabled === false ? (
                    <Badge>{t('plugins.list.disabled')}</Badge>
                  ) : null}
                  {plugin.trusted === false ? (
                    <Badge variant="warning">{t('plugins.list.untrusted')}</Badge>
                  ) : null}
                  {version.listBadge === 'notInstalled' ? (
                    <Badge variant="warning">{t('plugins.list.notInstalled')}</Badge>
                  ) : null}
                  {version.listBadge === 'versionMismatch' ? (
                    <Badge variant="warning">{t('plugins.list.versionMismatch')}</Badge>
                  ) : null}
                </div>
                {description ? (
                  <Tip
                    className="mt-0.5 line-clamp-1 truncate text-meta text-secondary"
                    label={description}
                  >
                    {description}
                  </Tip>
                ) : null}
              </div>
            </div>
          </ListRow>
        );
      })}
    </div>
  );
}
