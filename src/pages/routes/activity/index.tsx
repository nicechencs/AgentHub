import { useEffect, useMemo, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import { PageHeader } from '@/components/layout/PageHeader';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { ErrorState } from '@/components/shared/ErrorState';
import { PageRefreshButton } from '@/components/shared/PageRefreshButton';
import { useI18n } from '@/components/shared/LanguageProvider';
import { getLocalEntryStatus } from '@/lib/api/adapter';
import { ADAPTER_BRIDGE_STATUS_POLL_MS } from '@/pages/bridges/adapter-model';
import { useAdapterResources } from '@/pages/bridges/use-bridge-resources';
import { useRoutePoolState } from '@/pages/bridges/use-route-pool-state';
import { RoutesPane } from '@/pages/routes/RoutesPane';
import { activityRouteOptions } from '@/pages/routes/activity/inbound-feed-model';
import { ActivityMonitoringPanel } from '@/pages/routes/activity/ActivityMonitoringPanel';
import {
  monitoredLocalProfiles,
  resolveActivityPageSnapshot,
} from '@/pages/routes/activity/activity-view-model';

export default function RoutesActivityPage() {
  const { t } = useI18n();
  const [searchParams, setSearchParams] = useSearchParams();
  const routeId = searchParams.get('route');
  const {
    profiles,
    bridgeStatuses,
    profileState,
    errors,
    loading,
    reload,
  } = useAdapterResources();
  const { defaultPools } = useRoutePoolState({ profiles, detailTarget: null });
  const [localEntryStatuses, setLocalEntryStatuses] = useState<
    import('@/lib/backend/contracts/adapter').AdapterBridgeRuntimeStatus[]
  >([]);
  const [unauthenticatedTraces, setUnauthenticatedTraces] = useState<
    import('@/lib/backend/contracts/adapter').AdapterBridgeRouteTrace[]
  >([]);

  useEffect(() => {
    let cancelled = false;
    let received = false;
    const tick = () => {
      void getLocalEntryStatus()
        .then((status) => {
          if (cancelled) return;
          received = true;
          setLocalEntryStatuses(status.statuses ?? []);
          setUnauthenticatedTraces(status.unauthenticatedTraces ?? []);
        })
        .catch(() => {
          if (cancelled || received) return;
          setLocalEntryStatuses([]);
          setUnauthenticatedTraces([]);
        });
    };
    tick();
    const timer = window.setInterval(tick, ADAPTER_BRIDGE_STATUS_POLL_MS);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [profiles, loading]);

  const monitoredProfiles = useMemo(
    () => monitoredLocalProfiles(profiles, new Set(), defaultPools),
    [profiles, defaultPools],
  );
  const routeOptions = useMemo(
    () => activityRouteOptions(monitoredProfiles),
    [monitoredProfiles],
  );
  const snapshot = useMemo(
    () => resolveActivityPageSnapshot({
      profiles,
      bridgeStatuses,
      localEntryStatuses,
      unauthenticatedTraces,
      unauthenticatedSourceLabel: t('routes.activity.unauthenticatedSource'),
      pools: defaultPools,
      filter: 'all',
      routeId,
      profileState,
      loading,
    }),
    [
      profiles,
      bridgeStatuses,
      localEntryStatuses,
      unauthenticatedTraces,
      defaultPools,
      routeId,
      profileState,
      loading,
      t,
    ],
  );

  const setRouteFilter = (next: string) => {
    const params = new URLSearchParams(searchParams);
    if (!next) params.delete('route');
    else params.set('route', next);
    setSearchParams(params, { replace: true });
  };

  return (
    <RoutesPane>
      <PageHeader
        title={t('routes.activity.title')}
        description={t('routes.activity.description')}
      />
      <div className={pageRhythm.chromeRow}>
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
        <p className="min-w-0 flex-1 truncate text-meta text-muted">{t('routes.activity.scopeNote')}</p>
        <div className={pageRhythm.chromeActions}>
          <PageRefreshButton
            loading={loading}
            onClick={() => void reload()}
            label={t('routes.board.refresh')}
          />
        </div>
      </div>

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

      {snapshot.kind === 'error' ? (
        <ErrorState
          title={t('routes.loadError')}
          error={errors.profiles ?? t('routes.loadError')}
          onRetry={() => void reload()}
        />
      ) : (
        <ActivityMonitoringPanel snapshot={snapshot} pools={defaultPools} />
      )}
    </RoutesPane>
  );
}
