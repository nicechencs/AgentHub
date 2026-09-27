import { SegmentedControl } from '@/components/shared/SegmentedControl';
import { useI18n } from '@/components/shared/LanguageProvider';
import type { RouteSchedulePolicy } from '@/lib/backend/contracts/adapter';

/** Create-time schedule selector. Default priority_failover; only applies when a new default pool is created. */
export function PoolSchedulePolicyCreateControl({
  value,
  onChange,
  disabled,
}: {
  value: RouteSchedulePolicy;
  onChange: (next: RouteSchedulePolicy) => void;
  disabled?: boolean;
}) {
  const { t } = useI18n();
  return (
    <div className="space-y-1" data-pool-schedule-create>
      <div>
        <h3 className="text-sm font-medium">{t('routes.pool.schedule.createLabel')}</h3>
        <p className="text-meta text-muted">{t('routes.pool.schedule.createHint')}</p>
      </div>
      <SegmentedControl
        aria-label={t('routes.pool.schedule.createLabel')}
        size="sm"
        value={value}
        onChange={(next) => {
          if (next === value || disabled) return;
          onChange(next === 'round_robin' ? 'round_robin' : 'priority_failover');
        }}
        options={[
          {
            value: 'priority_failover',
            label: t('routes.pool.schedule.priorityFailover'),
            title: t('routes.pool.schedule.priorityFailoverHint'),
            disabled,
          },
          {
            value: 'round_robin',
            label: t('routes.pool.schedule.roundRobin'),
            title: t('routes.pool.schedule.roundRobinHint'),
            disabled,
          },
        ]}
      />
    </div>
  );
}
