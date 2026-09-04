import { Boxes } from 'lucide-react';
import { Link } from 'react-router-dom';
import { EmptyState } from '@/components/shared/EmptyState';
import { RouteTracePipelineLegend } from '@/components/shared/RouteTracePipelineLegend';
import { buttonVariants } from '@/components/ui/button';
import { TableSkeleton } from '@/components/ui/skeleton';
import { ROUTES_POOL_PATH } from '@/lib/routes-path';
import { cn } from '@/lib/utils';
import { useI18n } from '@/components/shared/LanguageProvider';
import { uniquePoolDisplayLabels, uniqueTraceUpstreamUrls } from '@/components/shared/route-trace-visual-model';
import { ActivityTraceList } from './ActivityTraceList';
import type { RouteTraceListItem } from '@/components/shared/RouteTraceList';
import type { ActivityPageSnapshot } from './activity-view-model';
import { activityTraceDisplayRow, selectedActivityTrace } from './activity-trace-summary-model';
import type { ActivityTraceKeyToken } from './activity-trace-list-model';

export function ActivityMonitoringPanel({
  snapshot,
  pools = [],
  tokens = [],
  activeId,
  onShowDetail,
}: {
  snapshot: ActivityPageSnapshot;
  pools?: readonly { members: readonly { displayLabel?: string }[] }[];
  tokens?: readonly ActivityTraceKeyToken[];
  activeId?: string | null;
  onShowDetail?: (row: RouteTraceListItem) => void;
}) {
  const { t } = useI18n();
  const activeRow = selectedActivityTrace(snapshot.feed, activeId);

  return (
    <div className="space-y-4" data-activity-monitoring>
      <RouteTracePipelineLegend
        row={activityTraceDisplayRow(activeRow ?? snapshot.feed[0])}
        poolLabels={uniquePoolDisplayLabels(pools)}
        upstreamUrls={uniqueTraceUpstreamUrls(snapshot.feed)}
      />
      <ActivityStatusBanner snapshot={snapshot} />
      {snapshot.kind === 'loading' ? (
        <TableSkeleton rows={6} cols={10} />
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
          tokens={tokens}
          activeId={activeId}
          onShowDetail={onShowDetail}
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
