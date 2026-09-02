import { Boxes } from 'lucide-react';
import { Link } from 'react-router-dom';
import { EmptyState } from '@/components/shared/EmptyState';
import { RouteTracePipelineLegend } from '@/components/shared/RouteTracePipelineLegend';
import { buttonVariants } from '@/components/ui/button';
import { TableSkeleton } from '@/components/ui/skeleton';
import { ROUTES_POOL_PATH } from '@/lib/bridges-path';
import { cn } from '@/lib/utils';
import { useI18n } from '@/components/shared/LanguageProvider';
import { ActivityTraceList } from './ActivityTraceList';
import type { ActivityPageSnapshot } from './activity-view-model';

export function ActivityMonitoringPanel({
  snapshot,
}: {
  snapshot: ActivityPageSnapshot;
}) {
  const { t } = useI18n();

  return (
    <div className="space-y-4" data-activity-monitoring>
      <p className="text-meta text-muted">{t('routes.activity.scopeNote')}</p>
      <RouteTracePipelineLegend />
      <ActivityStatusBanner snapshot={snapshot} />
      {snapshot.kind === 'loading' ? (
        <TableSkeleton rows={6} cols={6} />
      ) : snapshot.kind === 'noLogins' ? (
        <EmptyState
          icon={Boxes}
          title={t('routes.activity.noLoginsTitle')}
          description={t('routes.activity.noLoginsDescription')}
          action={(
            <Link
              to={ROUTES_POOL_PATH}
              className={cn(buttonVariants({ variant: 'outline', size: 'sm' }), 'mt-2')}
            >
              {t('routes.nav.goToList')}
            </Link>
          )}
        />
      ) : (
        <ActivityTraceList
          rows={snapshot.feed}
          emptyLabel={
            snapshot.kind === 'filteredEmpty'
              ? t('routes.activity.emptyFilteredTitle')
              : t('routes.activity.emptyTitle')
          }
        />
      )}
    </div>
  );
}

function ActivityStatusBanner({ snapshot }: { snapshot: ActivityPageSnapshot }) {
  const { t } = useI18n();
  if (snapshot.kind === 'loading' || snapshot.kind === 'error') return null;

  const key = (() => {
    switch (snapshot.kind) {
      case 'ready':
      case 'runningEmpty':
      case 'filteredEmpty':
        return 'routes.activity.statusListening';
      case 'notRunning':
        return 'routes.activity.statusStopped';
      case 'noRoutes':
        return 'routes.activity.statusAwaitingRoutes';
      case 'noLogins':
        return 'routes.activity.statusAwaitingLogins';
      default:
        return null;
    }
  })();
  if (!key) return null;

  return (
    <p className="rounded-card border border-border bg-subtle px-3 py-2 text-meta text-secondary">
      {t(key, {
        running: snapshot.runningCount,
        routes: snapshot.monitoredProfileIds.length,
      })}
    </p>
  );
}
