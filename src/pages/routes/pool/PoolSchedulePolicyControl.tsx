import { useState } from 'react';
import { SegmentedControl } from '@/components/shared/SegmentedControl';
import { useI18n } from '@/components/shared/LanguageProvider';
import { useToast } from '@/components/ui/toast';
import { setRoutePoolSchedulePolicy } from '@/lib/api/adapter';
import type { DefaultRoutePoolOverview, RouteSchedulePolicy } from '@/lib/backend/contracts/adapter';
import { localEndpointKindFromPool } from '@/lib/route-endpoints';
import { localEndpointKindLabel } from '@/pages/routes/shared/route-pool-view-model';

export function poolSchedulePolicy(
  pool: Pick<DefaultRoutePoolOverview, 'schedulePolicy'>,
): RouteSchedulePolicy {
  return pool.schedulePolicy === 'round_robin' ? 'round_robin' : 'priority_failover';
}

/** Per-pool schedule. Round robin stays inside one isomorphic group. */
export function PoolSchedulePolicies({
  pools,
  onChanged,
}: {
  pools: readonly DefaultRoutePoolOverview[];
  onChanged: () => void;
}) {
  const { t } = useI18n();
  const { toast } = useToast();
  const [busyId, setBusyId] = useState<string | null>(null);
  if (pools.length === 0) return null;
  return (
    <div className="mb-3 space-y-2" data-pool-schedule>
      <div>
        <h3 className="text-sm font-medium">{t('routes.pool.schedule.label')}</h3>
        <p className="text-meta text-muted">{t('routes.pool.schedule.hint')}</p>
      </div>
      {pools.map((pool) => {
        const kind = localEndpointKindFromPool(pool);
        const label = kind ? localEndpointKindLabel(kind, t) : pool.id;
        const value = poolSchedulePolicy(pool);
        const busy = busyId === pool.id;
        return (
          <div key={pool.id} className="flex min-w-0 flex-wrap items-center gap-2">
            <span className="min-w-0 truncate text-sm">{label}</span>
            <SegmentedControl
              aria-label={`${t('routes.pool.schedule.label')} · ${label}`}
              size="sm"
              value={value}
              onChange={(next) => {
                if (next === value || busyId) return;
                setBusyId(pool.id);
                void setRoutePoolSchedulePolicy(pool.id, next)
                  .then(() => {
                    toast({ title: t('routes.pool.schedule.saved'), variant: 'success' });
                    onChanged();
                  })
                  .catch((error: unknown) => {
                    toast({
                      title: t('routes.pool.schedule.saveFailed'),
                      description: error instanceof Error ? error.message : String(error),
                      variant: 'danger',
                    });
                  })
                  .finally(() => setBusyId(null));
              }}
              options={[
                {
                  value: 'priority_failover',
                  label: t('routes.pool.schedule.priorityFailover'),
                  title: t('routes.pool.schedule.priorityFailoverHint'),
                  disabled: busy,
                },
                {
                  value: 'round_robin',
                  label: t('routes.pool.schedule.roundRobin'),
                  title: t('routes.pool.schedule.roundRobinHint'),
                  disabled: busy,
                },
              ]}
            />
          </div>
        );
      })}
    </div>
  );
}
