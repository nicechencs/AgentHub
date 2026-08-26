import { AgentDot } from '@/components/shared/AgentDot';
import { ListRow } from '@/components/shared/ListRow';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Badge } from '@/components/ui/badge';
import { Tip } from '@/components/ui/tooltip';
import { agentDisplayName, AGENT_MAP } from '@/config/agents';
import type { PluginEntry } from '@/lib/backend/contracts/plugin-types';

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
    <div className="flex flex-col gap-2">
      {plugins.map((plugin) => {
        const active = plugin.id === activeId;
        const meta = AGENT_MAP[plugin.agent];
        return (
          <ListRow
            key={plugin.id}
            role="button"
            tabIndex={0}
            active={active}
            indicatorColor={meta?.color}
            className="cursor-pointer p-3"
            onClick={() => onOpen(plugin)}
            onKeyDown={(event) => {
              if (event.key === 'Enter' || event.key === ' ') {
                event.preventDefault();
                onOpen(plugin);
              }
            }}
          >
            <div className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1">
              {showAgent ? <AgentDot agentId={plugin.agent} /> : null}
              <Tip className="truncate text-body font-medium" label={plugin.name}>
                {plugin.name}
              </Tip>
              {plugin.version ? (
                <span className="font-mono text-meta text-muted">{plugin.version}</span>
              ) : null}
              {plugin.enabled === false ? (
                <Badge>{t('plugins.list.disabled')}</Badge>
              ) : plugin.enabled === true ? (
                <Badge variant="success">{t('plugins.list.enabled')}</Badge>
              ) : null}
              {plugin.trusted === false ? (
                <Badge variant="warning">{t('plugins.list.untrusted')}</Badge>
              ) : null}
              <span className="min-w-0 truncate text-meta text-secondary">
                {[
                  showAgent ? agentDisplayName(plugin.agent) : null,
                  plugin.marketplace,
                  plugin.scope,
                ]
                  .filter(Boolean)
                  .join(' · ')}
              </span>
            </div>
          </ListRow>
        );
      })}
    </div>
  );
}
