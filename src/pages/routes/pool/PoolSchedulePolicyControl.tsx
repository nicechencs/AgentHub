import { useRef, useState } from 'react';
import { SegmentedControl } from '@/components/shared/SegmentedControl';
import { useI18n } from '@/components/shared/LanguageProvider';
import { useToast } from '@/components/ui/toast';
import { setRoutePoolSchedulePolicy } from '@/lib/api/adapter';
import type { DefaultRoutePoolOverview, RouteSchedulePolicy } from '@/lib/backend/contracts/adapter';

export function poolSchedulePolicy(
  pool: Pick<DefaultRoutePoolOverview, 'schedulePolicy'>,
): RouteSchedulePolicy {
  return pool.schedulePolicy === 'round_robin' ? 'round_robin' : 'priority_failover';
}

/** Returns one policy only when every pool currently has the same policy. */
export function combinedPoolSchedulePolicy(
  pools: readonly Pick<DefaultRoutePoolOverview, 'schedulePolicy'>[],
): RouteSchedulePolicy | null {
  if (pools.length === 0) return null;
  const value = poolSchedulePolicy(pools[0]);
  return pools.every((pool) => poolSchedulePolicy(pool) === value) ? value : null;
}

export type PoolScheduleUpdateResult = {
  attempted: number;
  succeeded: number;
  failed: number;
};

export async function applyPoolSchedulePolicy(
  pools: readonly Pick<DefaultRoutePoolOverview, 'id'>[],
  schedulePolicy: RouteSchedulePolicy,
  update: (poolId: string, schedulePolicy: RouteSchedulePolicy) => Promise<unknown> = setRoutePoolSchedulePolicy,
): Promise<PoolScheduleUpdateResult> {
  const poolIds = [...new Set(pools.map((pool) => pool.id).filter((id) => id.trim()))];
  const results = await Promise.allSettled(
    poolIds.map((poolId) => Promise.resolve().then(() => update(poolId, schedulePolicy))),
  );
  const failed = results.filter((result) => result.status === 'rejected').length;
  return {
    attempted: poolIds.length,
    succeeded: poolIds.length - failed,
    failed,
  };
}

/** One shared schedule control. Saving still updates each pool independently. */
export function PoolSchedulePolicies({
  pools,
  onChanged,
}: {
  pools: readonly DefaultRoutePoolOverview[];
  onChanged: () => void;
}) {
  const { t } = useI18n();
  const { toast } = useToast();
  const [busy, setBusy] = useState(false);
  const pendingRef = useRef(false);
  if (pools.length === 0) return null;
  const value = combinedPoolSchedulePolicy(pools);
  return (
    <div className="mb-3 space-y-2" data-pool-schedule>
      <div>
        <h3 className="text-sm font-medium">{t('routes.pool.schedule.label')}</h3>
        <p className="text-meta text-muted">{t('routes.pool.schedule.hint')}</p>
      </div>
      {value === null ? (
        <p className="text-meta text-muted">{t('routes.pool.schedule.mixedHint')}</p>
      ) : null}
      <SegmentedControl
        aria-label={t('routes.pool.schedule.label')}
        size="sm"
        value={value ?? ''}
        onChange={(next) => {
          if (next !== 'priority_failover' && next !== 'round_robin') return;
          if (pendingRef.current || next === value) return;
          pendingRef.current = true;
          setBusy(true);
          void applyPoolSchedulePolicy(pools, next)
            .then((result) => {
              if (result.failed === 0) {
                toast({ title: t('routes.pool.schedule.saved'), variant: 'success' });
              } else {
                toast({
                  title: t('routes.pool.schedule.partialSaveFailed', { count: result.failed }),
                  variant: 'danger',
                });
              }
              onChanged();
            })
            .finally(() => {
              pendingRef.current = false;
              setBusy(false);
            });
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
}
