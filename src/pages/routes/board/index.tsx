import { useMemo } from 'react';
import { Link, useNavigate } from 'react-router-dom';
import { Boxes, Loader2 } from 'lucide-react';
import { PageHeader } from '@/components/layout/PageHeader';
import { PageSection } from '@/components/layout/PageSection';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { EmptyState } from '@/components/shared/EmptyState';
import { ErrorState } from '@/components/shared/ErrorState';
import { InboundRequestList } from '@/components/shared/InboundRequestList';
import { ListRow, ListRowBody, LIST_ROW_PAD } from '@/components/shared/ListRow';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
import { bridgesHrefForProfile, BRIDGES_PATH } from '@/lib/bridges-path';
import { cn } from '@/lib/utils';
import {
  adapterBridgeStateLabel,
  adapterBridgeUpstreamLabel,
} from '@/pages/bridges/adapter-labels';
import { adapterBridgeFleetSummary } from '@/pages/bridges/adapter-view-model';
import { useAdapterResources } from '@/pages/bridges/use-bridge-resources';
import { RoutesPane } from '@/pages/routes/RoutesPane';
import {
  buildRouteBoardStatusRows,
  mergeRecentInbound,
} from '@/pages/routes/board/board-view-model';

function stateTone(state: string | undefined): string {
  if (state === 'running') return 'bg-success';
  if (state === 'degraded' || state === 'error') return 'bg-warning';
  if (state === 'starting' || state === 'stopping') return 'bg-info';
  return 'bg-muted';
}

export default function RoutesBoardPage() {
  const { t } = useI18n();
  const navigate = useNavigate();
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
  const statusRows = useMemo(
    () => buildRouteBoardStatusRows(profiles, bridgeStatuses),
    [profiles, bridgeStatuses],
  );
  const recent = useMemo(
    () => mergeRecentInbound(profiles, bridgeStatuses, 20),
    [profiles, bridgeStatuses],
  );
  const fleet = adapterBridgeFleetSummary(bridges, bridgeStatuses, t);

  return (
    <RoutesPane>
      <PageHeader
        title={t('routes.board.title')}
        description={t('routes.board.description')}
      />
      <div className={pageRhythm.chromeRow}>
        <p className="min-w-0 truncate text-meta text-muted">
          {fleet?.label ?? t('routes.board.noFleet')}
        </p>
        <div className={pageRhythm.chromeActions}>
          <Button
            type="button"
            size="sm"
            variant="secondary"
            disabled={loading}
            onClick={() => void reload()}
          >
            {loading ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : null}
            {t('routes.board.refresh')}
          </Button>
        </div>
      </div>

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
              onClick={() => navigate(BRIDGES_PATH)}
            >
              {t('routes.nav.goToList')}
            </Button>
          }
        />
      ) : (
        <div className={pageRhythm.blocks}>
          <section className={pageRhythm.stackDense} aria-label={t('routes.board.statusSection')}>
            {statusRows.map((row) => (
              <ListRow key={row.profileId} className={LIST_ROW_PAD}>
                <ListRowBody
                  leading={
                    <span
                      className={cn('h-1.5 w-1.5 shrink-0 rounded-full', stateTone(row.state))}
                      aria-hidden
                    />
                  }
                  main={
                    <>
                      <span className="min-w-0 truncate text-sm font-medium text-primary">
                        {row.name}
                      </span>
                      <span className="text-meta text-secondary">
                        {adapterBridgeStateLabel(row.state, t)}
                      </span>
                      <span className="font-mono text-meta text-muted">
                        {row.endpoint ?? t('routes.pendingPort')}
                      </span>
                      <span className="text-meta text-muted">
                        {adapterBridgeUpstreamLabel(row.upstreamStatus, t)}
                      </span>
                    </>
                  }
                  actions={
                    <Link
                      to={bridgesHrefForProfile(row.profileId)}
                      className="text-meta text-secondary hover:text-primary"
                    >
                      {t('routes.detail')}
                    </Link>
                  }
                />
              </ListRow>
            ))}
          </section>

          <PageSection title={t('routes.inbound.title')}>
            {recent.length === 0 ? (
              <p className="text-sm text-muted">{t('routes.board.noRequests')}</p>
            ) : (
              <InboundRequestList rows={recent} />
            )}
          </PageSection>
        </div>
      )}
    </RoutesPane>
  );
}
