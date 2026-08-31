import { useMemo, useState } from 'react';
import { Loader2 } from 'lucide-react';
import { PageHeader } from '@/components/layout/PageHeader';
import { PageSection } from '@/components/layout/PageSection';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { ErrorState } from '@/components/shared/ErrorState';
import { useI18n } from '@/components/shared/LanguageProvider';
import { RouteEndpointTypeText } from '@/components/shared/RouteEndpointUrl';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import { Skeleton } from '@/components/ui/skeleton';
import { Tip } from '@/components/ui/tooltip';
import { useInstalledAgents } from '@/lib/hooks/useInstalledAgents';
import { cn } from '@/lib/utils';
import { routePoolSurfaceLabel } from '@/pages/bridges/route-pool-view-model';
import { useAdapterResources } from '@/pages/bridges/use-bridge-resources';
import { useRoutePoolState } from '@/pages/bridges/use-route-pool-state';
import { RoutesPane } from '@/pages/routes/RoutesPane';
import {
  BOARD_ROUTE_GRID,
  boardEndpointLoginTotals,
  buildBoardEndpointTypeRows,
  type BoardEndpointTypeRow,
} from '@/pages/routes/board/board-view-model';
import {
  poolSurfaceToUsageSurface,
  rememberedBoardUsageFilters,
  usageSurfaceToPoolSurface,
} from '@/pages/routes/board/board-usage-model';
import { BoardUsageSection } from '@/pages/routes/board/board-usage-section';

function BoardEndpointCard({
  row,
  selected,
  onSelect,
}: {
  row: BoardEndpointTypeRow;
  selected: boolean;
  onSelect: () => void;
}) {
  const { t } = useI18n();
  const label = routePoolSurfaceLabel(row.surface, t);
  const logins = t('routes.board.endpointLogins', {
    oauth: row.oauthCount,
    apikey: row.apikeyCount,
  });

  return (
    <Card
      role="button"
      tabIndex={0}
      aria-pressed={selected}
      aria-label={label}
      onClick={onSelect}
      onKeyDown={(event) => {
        if (event.key === 'Enter' || event.key === ' ') {
          event.preventDefault();
          onSelect();
        }
      }}
      className={cn(
        'cursor-pointer p-3 transition-colors hover:border-accent/40 hover:bg-hover/40',
        selected && 'border-accent bg-hover/40',
        row.oauthCount + row.apikeyCount === 0 && 'opacity-70',
      )}
    >
      <Tip className="block min-w-0 truncate font-mono text-sm font-medium" label={row.path}>
        <RouteEndpointTypeText endpointId={row.surface}>{row.path}</RouteEndpointTypeText>
      </Tip>
      <p className="mt-1.5 text-xs text-secondary">{logins}</p>
    </Card>
  );
}

function BoardRouteSkeleton({ count }: { count: number }) {
  const n = Math.max(1, count);
  return (
    <div className={BOARD_ROUTE_GRID}>
      {Array.from({ length: n }).map((_, i) => (
        <Card key={i} className="p-3">
          <Skeleton className="h-4 w-28" />
          <Skeleton className="mt-2 h-3 w-36" />
        </Card>
      ))}
    </div>
  );
}

export default function RoutesBoardPage() {
  const { t } = useI18n();
  const {
    profiles,
    profileState,
    errors,
    loading,
    reload,
  } = useAdapterResources();
  const { hiddenIds } = useInstalledAgents();
  const hiddenTargetIds = useMemo(() => new Set(hiddenIds), [hiddenIds]);
  const [usageRefreshKey, setUsageRefreshKey] = useState(0);
  const { defaultPools, loading: poolsLoading } = useRoutePoolState({
    profiles,
    detailTarget: null,
    reloadKey: usageRefreshKey,
  });
  const [surface, setSurface] = useState(() => rememberedBoardUsageFilters().surface);

  const endpointRows = useMemo(
    () => buildBoardEndpointTypeRows(defaultPools, hiddenTargetIds),
    [defaultPools, hiddenTargetIds],
  );
  const totals = boardEndpointLoginTotals(endpointRows);
  const selectedSurface = usageSurfaceToPoolSurface(surface);
  const selectedRow = endpointRows.find((row) => row.surface === selectedSurface) ?? null;
  const fleetLabel = totals.oauth + totals.apikey > 0
    ? t('routes.board.fleetLogins', { oauth: totals.oauth, apikey: totals.apikey })
    : t('routes.board.description');
  const loginHint = selectedRow
    ? t('routes.board.endpointLoginsHint', {
      oauth: selectedRow.oauthCount,
      apikey: selectedRow.apikeyCount,
    })
    : t('routes.board.endpointLoginsHintAll', {
      oauth: totals.oauth,
      apikey: totals.apikey,
    });
  const pageLoading = loading || poolsLoading;
  const showStatusSkeleton = pageLoading && defaultPools.length === 0;

  return (
    <RoutesPane>
      <PageHeader
        title={t('routes.board.title')}
        description={fleetLabel}
        descriptionTip={t('routes.board.descriptionTip')}
      />

      {profileState === 'error' ? (
        <ErrorState
          title={t('routes.loadError')}
          error={errors.profiles ?? t('routes.loadError')}
          onRetry={() => {
            void reload();
            setUsageRefreshKey((key) => key + 1);
          }}
        />
      ) : (
        <div className={pageRhythm.blocks}>
          <PageSection first aria-label={t('routes.board.statusSection')}>
            {showStatusSkeleton ? (
              <BoardRouteSkeleton count={3} />
            ) : (
              <div className={BOARD_ROUTE_GRID}>
                {endpointRows.map((row) => (
                  <BoardEndpointCard
                    key={row.surface}
                    row={row}
                    selected={selectedSurface === row.surface}
                    onSelect={() => {
                      setSurface((current) => (
                        usageSurfaceToPoolSurface(current) === row.surface
                          ? 'all'
                          : poolSurfaceToUsageSurface(row.surface)
                      ));
                    }}
                  />
                ))}
              </div>
            )}
            <div className="mt-3 flex items-center gap-2">
              <p className="min-w-0 flex-1 text-sm text-secondary">{loginHint}</p>
              <Button
                type="button"
                size="sm"
                variant="secondary"
                className="ml-auto shrink-0"
                disabled={pageLoading}
                onClick={() => {
                  void reload();
                  setUsageRefreshKey((key) => key + 1);
                }}
              >
                {pageLoading ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : null}
                {t('routes.board.refresh')}
              </Button>
            </div>
          </PageSection>

          <BoardUsageSection
            profiles={profiles}
            hiddenTargetIds={hiddenTargetIds}
            pools={defaultPools}
            refreshKey={usageRefreshKey}
            surface={surface}
          />
        </div>
      )}
    </RoutesPane>
  );
}
