import { useMemo } from 'react';
import { Activity, Boxes } from 'lucide-react';
import { useNavigate, useSearchParams } from 'react-router-dom';
import { PageHeader } from '@/components/layout/PageHeader';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { EmptyState } from '@/components/shared/EmptyState';
import { ErrorState } from '@/components/shared/ErrorState';
import { PageRefreshButton } from '@/components/shared/PageRefreshButton';
import { RouteTraceList } from '@/components/shared/RouteTraceList';
import { SegmentedControl } from '@/components/shared/SegmentedControl';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
import { ROUTES_POOL_PATH } from '@/lib/bridges-path';
import { useAdapterResources } from '@/pages/bridges/use-bridge-resources';
import { RoutesPane } from '@/pages/routes/RoutesPane';
import {
  activityRouteOptions,
  parseActivityFilter,
  type InboundFeedFilter,
} from '@/pages/routes/activity/inbound-feed-model';
import {
  buildRouteTraceFeed,
} from '@/pages/routes/activity/route-trace-feed-model';
import { activityHref } from '@/pages/routes/board/board-view-model';

export default function RoutesActivityPage() {
  const { t } = useI18n();
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();
  const filter = parseActivityFilter(searchParams.get('filter'));
  const routeId = searchParams.get('route');
  const {
    profiles,
    bridgeStatuses,
    profileState,
    errors,
    loading,
    reload,
  } = useAdapterResources();

  const bridges = useMemo(
    () => profiles.filter((profile) => profile.route === 'local_bridge'),
    [profiles],
  );
  const routeOptions = useMemo(() => activityRouteOptions(bridges), [bridges]);
  const feed = useMemo(
    () => buildRouteTraceFeed(profiles, bridgeStatuses, filter, 20, routeId),
    [profiles, bridgeStatuses, filter, routeId],
  );
  const allCount = useMemo(
    () => buildRouteTraceFeed(profiles, bridgeStatuses, 'all', 100, routeId).length,
    [profiles, bridgeStatuses, routeId],
  );
  const failedCount = useMemo(
    () => buildRouteTraceFeed(profiles, bridgeStatuses, 'failed', 100, routeId).length,
    [profiles, bridgeStatuses, routeId],
  );

  const setFilter = (next: InboundFeedFilter) => {
    const params = new URLSearchParams(searchParams);
    if (next === 'all') params.delete('filter');
    else params.set('filter', next);
    setSearchParams(params, { replace: true });
  };

  const setRouteFilter = (next: string) => {
    const params = new URLSearchParams(searchParams);
    if (!next) params.delete('route');
    else params.set('route', next);
    setSearchParams(params, { replace: true });
  };

  const filteredEmpty = Boolean(routeId || filter === 'failed');

  return (
    <RoutesPane>
      <PageHeader
        title={t('routes.activity.title')}
        description={t('routes.activity.description')}
      />
      <div className={pageRhythm.chromeRow}>
        <SegmentedControl
          aria-label={t('routes.activity.filterAria')}
          value={filter}
          onChange={setFilter}
          options={[
            { value: 'all', label: t('routes.activity.filterAll'), count: allCount },
            { value: 'failed', label: t('routes.activity.filterFailed'), count: failedCount },
          ]}
        />
        {routeOptions.length > 0 ? (
          <label className="flex min-w-0 items-center gap-2 text-meta text-secondary">
            <span className="shrink-0">{t('routes.activity.routeFilterAria')}</span>
            <select
              className="h-7 max-w-[12rem] truncate rounded-btn border border-border bg-panel px-2 text-meta text-primary"
              aria-label={t('routes.activity.routeFilterAria')}
              value={routeId ?? ''}
              onChange={(event) => setRouteFilter(event.target.value)}
            >
              <option value="">{t('routes.activity.routeFilterAll')}</option>
              {routeOptions.map((option) => (
                <option key={option.id} value={option.id}>
                  {option.label}
                </option>
              ))}
            </select>
          </label>
        ) : null}
        <div className={pageRhythm.chromeActions}>
          <PageRefreshButton
            loading={loading}
            onClick={() => void reload()}
            label={t('routes.board.refresh')}
          />
        </div>
      </div>

      <p className="mb-3 text-meta text-muted">{t('routes.activity.scopeNote')}</p>
      {routeId ? (
        <p className="mb-3 text-meta">
          <button
            type="button"
            className="text-secondary hover:text-primary"
            onClick={() => setRouteFilter('')}
          >
            {t('routes.activity.clearRouteFilter')}
          </button>
        </p>
      ) : null}

      {profileState === 'error' ? (
        <ErrorState
          title={t('routes.loadError')}
          error={errors.profiles ?? t('routes.loadError')}
          onRetry={() => void reload()}
        />
      ) : bridges.length === 0 && !loading ? (
        <EmptyState
          icon={Boxes}
          title={t('routes.board.emptyTitle')}
          description={t('routes.board.emptyDescription')}
          action={
            <Button
              size="sm"
              variant="outline"
              className="mt-2"
              onClick={() => navigate(ROUTES_POOL_PATH)}
            >
              {t('routes.nav.goToList')}
            </Button>
          }
        />
      ) : feed.length === 0 ? (
        <EmptyState
          icon={Activity}
          title={
            filteredEmpty
              ? t('routes.activity.emptyFilteredTitle')
              : t('routes.activity.emptyTitle')
          }
          description={
            filteredEmpty
              ? t('routes.activity.emptyFilteredDescription')
              : t('routes.activity.emptyDescription')
          }
          action={
            filteredEmpty ? (
              <Button
                size="sm"
                variant="outline"
                className="mt-2"
                onClick={() => navigate(activityHref({}))}
              >
                {t('routes.activity.clearRouteFilter')}
              </Button>
            ) : undefined
          }
        />
      ) : (
        <RouteTraceList rows={feed} />
      )}
    </RoutesPane>
  );
}
