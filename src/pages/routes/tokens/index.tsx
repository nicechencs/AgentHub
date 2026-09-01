import { useMemo } from 'react';
import { useNavigate } from 'react-router-dom';
import { KeyRound } from 'lucide-react';
import { PageHeader } from '@/components/layout/PageHeader';
import { WorkbenchSplitPage } from '@/components/layout/SideSplit';
import { useSideSplit } from '@/components/layout/use-side-split';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { EmptyState } from '@/components/shared/EmptyState';
import { ErrorState } from '@/components/shared/ErrorState';
import { PageRefreshButton } from '@/components/shared/PageRefreshButton';
import { ListRow, ListRowBody, LIST_ROW_PAD } from '@/components/shared/ListRow';
import { useI18n } from '@/components/shared/LanguageProvider';
import { CopyableRouteEndpointUrl, RouteEndpointUrl } from '@/components/shared/RouteEndpointUrl';
import { Button } from '@/components/ui/button';
import { useInstalledAgents } from '@/lib/hooks/useInstalledAgents';
import { ROUTES_POOL_PATH } from '@/lib/bridges-path';
import { localEndpointBrandAgentId } from '@/lib/route-endpoints';
import { ROUTES_INSPECT_WIDTH_KEY } from '@/pages/bridges/route-inspect';
import { useAdapterResources } from '@/pages/bridges/use-bridge-resources';
import { useRoutePoolState } from '@/pages/bridges/use-route-pool-state';
import { buildLocalEntryControl } from '@/pages/routes/board/board-view-model';
import { RoutesPane } from '@/pages/routes/RoutesPane';
import { TokenDetailPanel } from './TokenDetailPanel';
import { tokenEndpointParts } from './token-detail-model';
import { buildLocalTokenRows, tokenTypeLabel } from './tokens-model';

export default function RoutesTokensPage() {
  const { t } = useI18n();
  const navigate = useNavigate();
  const { hiddenIds } = useInstalledAgents();
  const hiddenTargetIds = useMemo(() => new Set(hiddenIds), [hiddenIds]);
  const {
    profiles,
    bridgeStatuses,
    errors,
    profileState,
    loading,
    reload,
  } = useAdapterResources();
  const {
    chatCompletionsShared,
    defaultPools,
    loading: poolsLoading,
  } = useRoutePoolState({
    profiles,
    detailTarget: null,
  });
  const inspect = useSideSplit<string>({ storageKey: ROUTES_INSPECT_WIDTH_KEY });
  const localEntry = useMemo(
    () => buildLocalEntryControl(profiles, bridgeStatuses, hiddenTargetIds, defaultPools),
    [bridgeStatuses, defaultPools, hiddenTargetIds, profiles],
  );
  const rows = useMemo(
    () => {
      const next = buildLocalTokenRows(
        profiles,
        bridgeStatuses,
        errors.bridgeStatuses,
        defaultPools,
        chatCompletionsShared,
      );
      if (profileState !== 'error') return next;
      return next.map((row) => ({
        ...row,
        token: null,
        maskedToken: null,
        unavailable: true,
      }));
    },
    [
      bridgeStatuses,
      chatCompletionsShared,
      defaultPools,
      errors.bridgeStatuses,
      profileState,
      profiles,
    ],
  );

  const detailRow = inspect.target
    ? rows.find((row) => row.id === inspect.target) ?? null
    : null;
  const pageLoading = loading || poolsLoading;

  const list = (
    <RoutesPane>
      <PageHeader
        title={t('routes.tokens.title')}
        description={t('routes.tokens.description')}
      />
      <div className={pageRhythm.chromeRow}>
        <p className="min-w-0 truncate text-meta text-muted">{
          !localEntry.running
            && localEntry.profileIds.length === 0
            && localEntry.hasEnrolledLogins
            ? t('routes.board.entryNeedRoute')
            : t('routes.tokens.scopeNote')
        }</p>
        <div className={pageRhythm.chromeActions}>
          <PageRefreshButton
            loading={pageLoading}
            onClick={() => void reload()}
            label={t('routes.board.refresh')}
          />
        </div>
      </div>

      {profileState === 'error' && rows.length === 0 && !pageLoading ? (
        <ErrorState
          title={t('routes.runtime.unavailable')}
          error={errors.profiles ?? new Error(t('routes.runtime.unavailable'))}
          onRetry={() => void reload()}
        />
      ) : rows.length === 0 && !pageLoading ? (
        <EmptyState
          icon={KeyRound}
          title={t('routes.tokens.emptyTitle')}
          description={t('routes.tokens.emptyDescription')}
          action={
            <Button
              size="sm"
              variant="outline"
              className="mt-2"
              onClick={() => navigate(ROUTES_POOL_PATH)}
            >
              {t('routes.nav.goToPool')}
            </Button>
          }
        />
      ) : (
        <div className={pageRhythm.stackDense}>
          {rows.map((row) => {
            const typeLabel = tokenTypeLabel(row, t);
            const endpoint = tokenEndpointParts(row);
            const brandAgentId = localEndpointBrandAgentId(row.kind);
            return (
              <ListRow
                key={row.id}
                className={LIST_ROW_PAD}
                active={inspect.target === row.id}
                onOpen={() => inspect.open(row.id)}
                aria-label={t('routes.tokens.openDetailAria', { name: typeLabel })}
              >
                <ListRowBody
                  main={
                    <>
                      <span className="min-w-0 truncate text-sm font-medium text-primary">
                        {typeLabel}
                      </span>
                      {endpoint.portPending ? (
                        <RouteEndpointUrl
                          path={row.path}
                          port={null}
                          host={endpoint.host}
                          endpointId={endpoint.endpointId}
                          brandAgentId={brandAgentId}
                          className="text-meta"
                        />
                      ) : (
                        <CopyableRouteEndpointUrl
                          path={row.path}
                          port={Number(endpoint.portLabel)}
                          host={endpoint.host}
                          endpointId={endpoint.endpointId}
                          brandAgentId={brandAgentId}
                          className="text-meta"
                        />
                      )}
                      {row.unavailable ? (
                        <span className="text-meta text-muted">{t('routes.runtime.unavailable')}</span>
                      ) : row.maskedToken ? (
                        <span className="min-w-0 truncate font-mono text-meta text-secondary">
                          {row.maskedToken}
                        </span>
                      ) : null}
                    </>
                  }
                />
              </ListRow>
            );
          })}
        </div>
      )}
    </RoutesPane>
  );

  return (
    <WorkbenchSplitPage
      split={inspect}
      resizeAria={t('common.resizeSidePanel')}
      panel={detailRow ? (
        <TokenDetailPanel
          row={detailRow}
          width={inspect.paneWidth}
          onClose={() => inspect.close()}
        />
      ) : null}
    >
      {list}
    </WorkbenchSplitPage>
  );
}
