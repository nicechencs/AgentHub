import { agentDisplayName } from '@/config/agents';
import { AgentDot } from '@/components/shared/AgentDot';
import { ListRow, LIST_ROW_PAD } from '@/components/shared/ListRow';
import { StatusPin } from '@/components/shared/StatusPin';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Switch } from '@/components/ui/switch';
import { connectionKindLabel } from '@/lib/connection-kind';
import { adapterStatusTextClass } from '@/pages/bridges/adapter-view-model';
import {
  poolAuthorizationStatusView,
  type PoolAuthorizationItem,
} from '@/pages/bridges/route-pool-view-model';
import { poolAuthorizationListChips } from './pool-authorization-detail';

export function PoolAuthorizationList({
  items,
  activeKey,
  togglingKey,
  onShowDetail,
  onEnabledChange,
}: {
  items: readonly PoolAuthorizationItem[];
  activeKey?: string | null;
  togglingKey?: string | null;
  onShowDetail?: (item: PoolAuthorizationItem) => void;
  onEnabledChange?: (item: PoolAuthorizationItem, enabled: boolean) => void;
}) {
  const { t } = useI18n();
  return (
    <div className="space-y-2">
      {items.map((item) => {
        const status = poolAuthorizationStatusView(item, t);
        const chips = poolAuthorizationListChips(item, t);
        return (
          <ListRow
            key={item.key}
            className={LIST_ROW_PAD}
            data-pool-authorization={item.key}
            active={activeKey === item.key}
            onOpen={onShowDetail ? () => onShowDetail(item) : undefined}
          >
            <div className="flex min-w-0 items-start gap-2">
              {item.canToggle ? (
                <Switch
                  checked={item.enabled !== false}
                  disabled={togglingKey === item.key}
                  onCheckedChange={(enabled) => onEnabledChange?.(item, enabled)}
                  aria-label={t('routes.pool.detail.enabled')}
                />
              ) : null}
              <div className="flex min-w-0 flex-1 flex-col gap-1">
                <div className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1 text-sm">
                  <StatusPin tone={status.tone} size="md" />
                  <AgentDot agentId={item.agentId} size="sm" title={null} />
                  <span className="truncate font-medium">{item.title}</span>
                  <span className="text-meta text-muted">{connectionKindLabel(item.kind, t)}</span>
                  <span className="text-meta text-secondary">{agentDisplayName(item.agentId)}</span>
                  <span className={adapterStatusTextClass(status.tone)}>{status.label}</span>
                </div>
                {chips.length > 0 ? (
                  <p className="text-meta text-muted">{chips.join(' · ')}</p>
                ) : null}
              </div>
            </div>
          </ListRow>
        );
      })}
    </div>
  );
}
