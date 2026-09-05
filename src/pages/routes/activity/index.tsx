import { useCallback, useEffect, useMemo, useState } from 'react';
import { Trash2 } from 'lucide-react';
import { useSearchParams } from 'react-router-dom';
import { PageHeader } from '@/components/layout/PageHeader';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { WorkbenchSplitPage } from '@/components/layout/SideSplit';
import { useSideSplit } from '@/components/layout/use-side-split';
import { ErrorState } from '@/components/shared/ErrorState';
import { PageRefreshButton } from '@/components/shared/PageRefreshButton';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { useToast } from '@/components/ui/toast';
import {
  deleteRouteTraces,
  getLocalGatewayStatus,
  listLocalTokens,
  queryRouteTraces,
} from '@/lib/api/adapter';
import type {
  AdapterBridgeRouteTrace,
  AdapterBridgeRuntimeStatus,
  LocalTokenRecord,
  RouteTracePage,
} from '@/lib/backend/contracts/adapter';
import { ADAPTER_BRIDGE_STATUS_POLL_MS } from '@/pages/routes/shared/adapter-model';
import { useAdapterResources } from '@/pages/routes/shared/use-bridge-resources';
import { useRoutePoolState } from '@/pages/routes/shared/use-route-pool-state';
import { localEndpointKindLabel } from '@/pages/routes/shared/route-pool-view-model';
import { activityRouteOptions } from '@/pages/routes/activity/inbound-feed-model';
import { ActivityMonitoringPanel } from '@/pages/routes/activity/ActivityMonitoringPanel';
import { ActivityTraceDetailPanel } from '@/pages/routes/activity/ActivityTraceDetailPanel';
import {
  isTraceQueryErrorEmpty,
  monitoredLocalProfiles,
  pageAfterTraceQueryFailure,
  resolveActivityPageSnapshot,
} from '@/pages/routes/activity/activity-view-model';
import { selectedActivityTrace } from '@/pages/routes/activity/activity-trace-summary-model';
import { decorateRouteTraceRows } from '@/pages/routes/activity/route-trace-feed-model';
import {
  ACTIVITY_PAGE_SIZE,
  activityEndpointKinds,
  activityKeyOptionLabel,
  clampActivityPage,
  parseActivityEndpointParam,
  parseActivityPageParam,
  resolveActivityKeyQuery,
} from '@/pages/routes/activity/activity-query-model';
import { StorageKey } from '@/lib/ui-preferences';

const ACTIVITY_PREVIEW_WIDTH_KEY = StorageKey.routesActivityPreviewWidth;

const EMPTY_PAGE: RouteTracePage = {
  rows: [],
  total: 0,
  offset: 0,
  limit: ACTIVITY_PAGE_SIZE,
};

export default function RoutesActivityPage() {
  const { t } = useI18n();
  const { toast } = useToast();
  const [searchParams, setSearchParams] = useSearchParams();
  const routeId = searchParams.get('route');
  const keyId = searchParams.get('key');
  const endpoint = parseActivityEndpointParam(searchParams.get('endpoint'));
  const page = parseActivityPageParam(searchParams.get('page'));
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
  const [localGatewayStatuses, setLocalGatewayStatuses] = useState<AdapterBridgeRuntimeStatus[]>([]);
  const [unauthenticatedTraces, setUnauthenticatedTraces] = useState<AdapterBridgeRouteTrace[]>([]);
  const [localTokens, setLocalTokens] = useState<LocalTokenRecord[]>([]);
  const [tokensReady, setTokensReady] = useState(false);
  const [tracePage, setTracePage] = useState<RouteTracePage>(EMPTY_PAGE);
  const [traceError, setTraceError] = useState<unknown | null>(null);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [deleteOpen, setDeleteOpen] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [refreshTick, setRefreshTick] = useState(0);

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
      })
      .finally(() => {
        if (!cancelled) setTokensReady(true);
      });
    return () => {
      cancelled = true;
    };
  }, [loading, profiles]);

  const keyQuery = useMemo(
    () => resolveActivityKeyQuery(keyId, localTokens),
    [keyId, localTokens],
  );

  useEffect(() => {
    if (keyId && !tokensReady) return;
    let cancelled = false;
    const offset = (page - 1) * ACTIVITY_PAGE_SIZE;
    const tick = () => {
      if (keyId && tokensReady && !keyQuery) {
        setTraceError(null);
        setTracePage({ ...EMPTY_PAGE, offset });
        return;
      }
      void queryRouteTraces({
        keyLast4: keyQuery?.keyLast4,
        poolId: keyQuery?.poolId,
        endpointKind: endpoint,
        routeId,
        offset,
        limit: ACTIVITY_PAGE_SIZE,
      })
        .then((next) => {
          if (cancelled) return;
          setTraceError(null);
          setTracePage(next);
        })
        .catch((error: unknown) => {
          if (cancelled) return;
          setTraceError(error ?? true);
          setTracePage((prev) => pageAfterTraceQueryFailure(prev, { ...EMPTY_PAGE, offset }));
        });
    };
    tick();
    const timer = window.setInterval(tick, ADAPTER_BRIDGE_STATUS_POLL_MS);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [endpoint, keyId, keyQuery, page, routeId, tokensReady, refreshTick]);

  const monitoredProfiles = useMemo(
    () => monitoredLocalProfiles(profiles, new Set(), defaultPools),
    [profiles, defaultPools],
  );
  const routeOptions = useMemo(
    () => activityRouteOptions(monitoredProfiles),
    [monitoredProfiles],
  );
  const labeledRows = useMemo(
    () => decorateRouteTraceRows(
      tracePage.rows,
      monitoredProfiles,
      t('routes.activity.unauthenticatedSource'),
    ),
    [tracePage.rows, monitoredProfiles, t],
  );
  const baseSnapshot = useMemo(
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
  const snapshot = useMemo(
    () => ({ ...baseSnapshot, feed: labeledRows }),
    [baseSnapshot, labeledRows],
  );
  const detailRow = selectedActivityTrace(snapshot.feed, inspect.target);
  const hasFilters = Boolean(keyId || endpoint || routeId);
  const safePage = clampActivityPage(page, tracePage.total, ACTIVITY_PAGE_SIZE);

  useEffect(() => {
    if (page === safePage) return;
    const params = new URLSearchParams(searchParams);
    if (safePage <= 1) params.delete('page');
    else params.set('page', String(safePage));
    setSearchParams(params, { replace: true });
  }, [page, safePage, searchParams, setSearchParams]);

  useEffect(() => {
    if (!inspect.target) return;
    if (!snapshot.feed.some((row) => row.requestId === inspect.target)) inspect.close();
    // eslint-disable-next-line react-hooks/exhaustive-deps -- close when the selected request leaves the feed
  }, [inspect.target, snapshot.feed]);

  useEffect(() => {
    const visible = new Set(labeledRows.map((row) => row.requestId));
    setSelectedIds((prev) => {
      const next = new Set([...prev].filter((id) => visible.has(id)));
      return next.size === prev.size ? prev : next;
    });
  }, [labeledRows]);

  const patchParams = useCallback((patch: (params: URLSearchParams) => void) => {
    const params = new URLSearchParams(searchParams);
    patch(params);
    setSearchParams(params, { replace: true });
  }, [searchParams, setSearchParams]);

  const setRouteFilter = (next: string) => {
    patchParams((params) => {
      if (!next) params.delete('route');
      else params.set('route', next);
      params.delete('page');
    });
  };
  const setKeyFilter = (next: string) => {
    patchParams((params) => {
      if (!next) params.delete('key');
      else params.set('key', next);
      params.delete('page');
    });
  };
  const setEndpointFilter = (next: string) => {
    patchParams((params) => {
      if (!next) params.delete('endpoint');
      else params.set('endpoint', next);
      params.delete('page');
    });
  };
  const setPage = (next: number) => {
    patchParams((params) => {
      if (next <= 1) params.delete('page');
      else params.set('page', String(next));
    });
  };

  const toggleRow = (id: string) => {
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };
  const togglePage = () => {
    const ids = labeledRows.map((row) => row.requestId);
    setSelectedIds((prev) => {
      const allSelected = ids.length > 0 && ids.every((id) => prev.has(id));
      const next = new Set(prev);
      if (allSelected) {
        for (const id of ids) next.delete(id);
      } else {
        for (const id of ids) next.add(id);
      }
      return next;
    });
  };

  const handleBatchDelete = async () => {
    const ids = [...selectedIds];
    if (ids.length === 0) return;
    setDeleting(true);
    try {
      const result = await deleteRouteTraces(ids);
      toast({ title: t('routes.activity.deleted', { n: result.deleted }) });
      setSelectedIds(new Set());
      setDeleteOpen(false);
      setRefreshTick((value) => value + 1);
    } catch {
      toast({ title: t('routes.activity.deleteFailed'), variant: 'danger' });
    } finally {
      setDeleting(false);
    }
  };

  const selectClass = 'h-7 max-w-[12rem] truncate rounded-btn border border-border bg-panel px-2 text-meta text-primary';

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
        <label className="flex min-w-0 items-center gap-2 text-meta text-secondary">
          <span className="shrink-0">{t('routes.activity.keyFilterAria')}</span>
          <select
            className={selectClass}
            aria-label={t('routes.activity.keyFilterAria')}
            value={keyId ?? ''}
            onChange={(event) => setKeyFilter(event.target.value)}
          >
            <option value="">{t('routes.activity.keyFilterAll')}</option>
            {localTokens.map((token) => (
              <option key={token.id} value={token.id}>
                {activityKeyOptionLabel(token)}
              </option>
            ))}
          </select>
        </label>
        <label className="flex min-w-0 items-center gap-2 text-meta text-secondary">
          <span className="shrink-0">{t('routes.activity.endpointFilterAria')}</span>
          <select
            className={selectClass}
            aria-label={t('routes.activity.endpointFilterAria')}
            value={endpoint ?? ''}
            onChange={(event) => setEndpointFilter(event.target.value)}
          >
            <option value="">{t('routes.activity.endpointFilterAll')}</option>
            {activityEndpointKinds().map((kind) => (
              <option key={kind} value={kind}>
                {localEndpointKindLabel(kind, t)}
              </option>
            ))}
          </select>
        </label>
        {routeOptions.length > 0 ? (
          <label className="flex min-w-0 items-center gap-2 text-meta text-secondary">
            <span className="shrink-0">{t('routes.activity.routeFilterAria')}</span>
            <select
              className={selectClass}
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
          {selectedIds.size > 0 ? (
            <Button
              type="button"
              variant="danger"
              size="sm"
              onClick={() => setDeleteOpen(true)}
            >
              <Trash2 className="h-3.5 w-3.5" />
              {t('routes.activity.deleteSelected')}
            </Button>
          ) : null}
          <PageRefreshButton
            loading={loading}
            onClick={() => {
              void reload();
              setRefreshTick((value) => value + 1);
            }}
            label={t('routes.board.refresh')}
          />
        </div>
      </div>

      {snapshot.kind === 'error' ? (
        <ErrorState
          title={t('routes.loadError')}
          error={errors.profiles ?? t('routes.loadError')}
          onRetry={() => void reload()}
        />
      ) : isTraceQueryErrorEmpty(traceError, labeledRows.length) ? (
        <ErrorState
          title={t('routes.loadError')}
          error={traceError instanceof Error ? traceError : t('routes.loadError')}
          onRetry={() => setRefreshTick((value) => value + 1)}
        />
      ) : (
        <ActivityMonitoringPanel
          snapshot={snapshot}
          pools={defaultPools}
          tokens={localTokens}
          activeId={inspect.target}
          onShowDetail={(row) => inspect.open(row.requestId)}
          selectedIds={selectedIds}
          onToggleRow={toggleRow}
          onTogglePage={togglePage}
          page={safePage}
          total={tracePage.total}
          pageSize={ACTIVITY_PAGE_SIZE}
          onPageChange={setPage}
          filtered={hasFilters}
          traceError={traceError}
          onRetryTraces={() => setRefreshTick((value) => value + 1)}
        />
      )}

      <Dialog open={deleteOpen} onOpenChange={setDeleteOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t('routes.activity.deleteConfirmTitle', { n: selectedIds.size })}</DialogTitle>
            <DialogDescription>{t('routes.activity.deleteConfirmDesc')}</DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="secondary" disabled={deleting} onClick={() => setDeleteOpen(false)}>
              {t('common.cancel')}
            </Button>
            <Button variant="danger" disabled={deleting} onClick={() => void handleBatchDelete()}>
              {t('common.delete')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </WorkbenchSplitPage>
  );
}
