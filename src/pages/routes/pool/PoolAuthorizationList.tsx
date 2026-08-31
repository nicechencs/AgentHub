import { agentDisplayName } from '@/config/agents';
import { AgentDot } from '@/components/shared/AgentDot';
import { ListRow, LIST_ROW_PAD } from '@/components/shared/ListRow';
import { StatusPin } from '@/components/shared/StatusPin';
import { useI18n } from '@/components/shared/LanguageProvider';
import { connectionKindLabel } from '@/lib/connection-kind';
import { adapterStatusTextClass } from '@/pages/bridges/adapter-view-model';
import {
  poolAuthorizationStatusView,
  type PoolAuthorizationItem,
} from '@/pages/bridges/route-pool-view-model';

export function PoolAuthorizationList({
  items,
}: {
  items: readonly PoolAuthorizationItem[];
}) {
  const { t } = useI18n();
  return (
    <div className="space-y-2">
      {items.map((item) => {
        const status = poolAuthorizationStatusView(item, t);
        return (
          <ListRow key={item.key} className={LIST_ROW_PAD} data-pool-authorization={item.key}>
            <div className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1 text-sm">
              <StatusPin tone={status.tone} size="md" />
              <AgentDot agentId={item.agentId} size="sm" title={null} />
              <span className="truncate font-medium">{item.title}</span>
              <span className="text-meta text-muted">{connectionKindLabel(item.kind, t)}</span>
              <span className="text-meta text-secondary">{agentDisplayName(item.agentId)}</span>
              <span className={adapterStatusTextClass(status.tone)}>{status.label}</span>
            </div>
          </ListRow>
        );
      })}
    </div>
  );
}
