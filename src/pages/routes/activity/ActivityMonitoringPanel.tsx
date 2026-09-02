import { Activity, Boxes, Radio } from 'lucide-react';
import { Link, useNavigate } from 'react-router-dom';
import { EmptyState } from '@/components/shared/EmptyState';
import { RouteTraceList } from '@/components/shared/RouteTraceList';
import { RouteTracePipelineLegend } from '@/components/shared/RouteTracePipelineLegend';
import { Button, buttonVariants } from '@/components/ui/button';
import { ROUTES_BOARD_PATH, ROUTES_POOL_PATH } from '@/lib/bridges-path';
import { cn } from '@/lib/utils';
import { useI18n } from '@/components/shared/LanguageProvider';
import { activityHref } from '@/pages/routes/board/board-view-model';
import type { ActivityPageSnapshot } from './activity-view-model';

export function ActivityMonitoringPanel({
  snapshot,
}: {
  snapshot: ActivityPageSnapshot;
}) {
  const { t } = useI18n();
  const navigate = useNavigate();

  return (
    <div className="space-y-4" data-activity-monitoring>
      <p className="text-meta text-muted">{t('routes.activity.scopeNote')}</p>
      <RouteTracePipelineLegend />
      <ActivityStatusBanner snapshot={snapshot} />
      {snapshot.kind === 'ready' ? (
        <RouteTraceList rows={snapshot.feed} />
      ) : snapshot.kind === 'filteredEmpty' ? (
        <EmptyState
          icon={Activity}
          title={t('routes.activity.emptyFilteredTitle')}
          description={t('routes.activity.emptyFilteredDescription')}
          action={(
            <Button
              size="sm"
              variant="outline"
              className="mt-2"
              onClick={() => navigate(activityHref({}))}
            >
              {t('routes.activity.clearRouteFilter')}
            </Button>
          )}
        />
      ) : snapshot.kind === 'runningEmpty' ? (
        <EmptyState
          icon={Radio}
          title={t('routes.activity.runningEmptyTitle')}
          description={t('routes.activity.runningEmptyDescription')}
        />
      ) : snapshot.kind === 'notRunning' ? (
        <EmptyState
          icon={Radio}
          title={t('routes.activity.notRunningTitle')}
          description={t('routes.activity.notRunningDescription')}
          action={(
            <Link
              to={ROUTES_BOARD_PATH}
              className={cn(buttonVariants({ variant: 'outline', size: 'sm' }), 'mt-2')}
            >
              {t('routes.activity.goToBoard')}
            </Link>
          )}
        />
      ) : snapshot.kind === 'noRoutes' ? (
        <EmptyState
          icon={Boxes}
          title={t('routes.activity.noRoutesTitle')}
          description={t('routes.activity.noRoutesDescription')}
          action={(
            <Link
              to={ROUTES_BOARD_PATH}
              className={cn(buttonVariants({ variant: 'outline', size: 'sm' }), 'mt-2')}
            >
              {t('routes.activity.goToBoard')}
            </Link>
          )}
        />
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
      ) : null}
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
