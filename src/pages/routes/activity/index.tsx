import { useEffect, useMemo, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import { PageHeader } from '@/components/layout/PageHeader';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { WorkbenchSplitPage } from '@/components/layout/SideSplit';
import { useSideSplit } from '@/components/layout/use-side-split';
import { ErrorState } from '@/components/shared/ErrorState';
import { PageRefreshButton } from '@/components/shared/PageRefreshButton';
import { useI18n } from '@/components/shared/LanguageProvider';
import { getLocalGatewayStatus, listLocalTokens } from '@/lib/api/adapter';
import type { LocalTokenRecord } from '@/lib/backend/contracts/adapter';
import { ADAPTER_BRIDGE_STATUS_POLL_MS } from '@/pages/routes/shared/adapter-model';
import { useAdapterResources } from '@/pages/routes/shared/use-bridge-resources';
import { useRoutePoolState } from '@/pages/routes/shared/use-route-pool-state';
import { activityRouteOptions } from '@/pages/routes/activity/inbound-feed-model';
import { ActivityMonitoringPanel } from '@/pages/routes/activity/ActivityMonitoringPanel';
import { ActivityTraceDetailPanel } from '@/pages/routes/activity/ActivityTraceDetailPanel';
import {
  monitoredLocalProfiles,
  resolveActivityPageSnapshot,
} from '@/pages/routes/activity/activity-view-model';
import { selectedActivityTrace } from '@/pages/routes/activity/activity-trace-summary-model';
import { StorageKey } from '@/lib/ui-preferences';

const ACTIVITY_PREVIEW_WIDTH_KEY = StorageKey.routesActivityPreviewWidth;

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
  const inspect = useSideSplit<string>({ storageKey: ACTIVITY_PREVIEW_WIDTH_KEY });
  const [localGatewayStatuses, setLocalGatewayStatuses] = useState<
    import('@/lib/backend/contracts/adapter').AdapterBridgeRuntimeStatus[]
  >([]);
  const [unauthenticatedTraces, setUnauthenticatedTraces] = useState<
    import('@/lib/backend/contracts/adapter').AdapterBridgeRouteTrace[]
  >([]);
  const [localTokens, setLocalTokens] = useState<LocalTokenRecord[]>([]);

  useEffect(() => {
    let cancelled = false;
    let received = false;
    const tick = () => {
      void getLocalGatewayStatus()
        .then((status) => {
          if (cancelled) return;
          received = true;
          setLocalGatewayStatuses(status.statuses ?? []);
          setUnauthenticatedTraces(status.unauthenticatedTraces ?? []);
        })
        .catch(() => {
          if (cancelled || received) return;
          setLocalGatewayStatuses([]);
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

  useEffect(() => {
    let cancelled = false;
    void listLocalTokens()
      .then((rows) => {
        if (!cancelled) setLocalTokens(rows);
      })
      .catch(() => {
        if (!cancelled) setLocalTokens([]);
      });
    return () => {
      cancelled = true;
    };
  }, [loading, profiles]);

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
      localGatewayStatuses,
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
      localGatewayStatuses,
      unauthenticatedTraces,
      defaultPools,
      routeId,
      profileState,
      loading,
      t,
    ],
  );
  const detailRow = selectedActivityTrace(snapshot.feed, inspect.target);

  useEffect(() => {
    if (!inspect.target) return;
    if (!snapshot.feed.some((row) => row.requestId === inspect.target)) inspect.close();
    // eslint-disable-next-line react-hooks/exhaustive-deps -- close when the selected request leaves the feed
  }, [inspect.target, snapshot.feed]);

  const setRouteFilter = (next: string) => {
    const params = new URLSearchParams(searchParams);
    if (!next) params.delete('route');
    else params.set('route', next);
    setSearchParams(params, { replace: true });
  };

  return (
    <WorkbenchSplitPage
      split={inspect}
      resizeAria={t('common.resizeSidePanel')}
      panel={detailRow ? (
        <ActivityTraceDetailPanel
          row={detailRow}
          tokens={localTokens}
          width={inspect.paneWidth}
          onClose={() => inspect.close()}
        />
      ) : null}
    >
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
        <ActivityMonitoringPanel
          snapshot={snapshot}
          pools={defaultPools}
          tokens={localTokens}
          activeId={inspect.target}
          onShowDetail={(row) => inspect.open(row.requestId)}
        />
      )}
    </WorkbenchSplitPage>
  );
}
