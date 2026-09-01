import { useEffect, useMemo, useState } from 'react';
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
import { Button } from '@/components/ui/button';
import { useToast } from '@/components/ui/toast';
import { useInstalledAgents } from '@/lib/hooks/useInstalledAgents';
import { ROUTES_POOL_PATH } from '@/lib/bridges-path';
import { localEndpointBrandAgentId } from '@/lib/route-endpoints';
import { agentCssVar } from '@/styles/tokens';
import { WriteClientConfigDialog } from '@/pages/bridges/WriteClientConfigDialog';
import { buildRouteGraph } from '@/pages/bridges/route-graph-model';
import {
  ROUTES_INSPECT_WIDTH_KEY,
  type WriteTarget,
} from '@/pages/bridges/route-inspect';
import {
  localEndpointKindLabel,
} from '@/pages/bridges/route-pool-view-model';
import { useAdapterResources } from '@/pages/bridges/use-bridge-resources';
import { useRoutePoolState } from '@/pages/bridges/use-route-pool-state';
import { buildLocalEntryControl } from '@/pages/routes/board/board-view-model';
import { RoutesPane } from '@/pages/routes/RoutesPane';
import { buildLocalTokenRows } from './tokens-model';

export default function RoutesTokensPage() {
  const { t } = useI18n();
  const navigate = useNavigate();
  const { toast } = useToast();
  const { hiddenIds } = useInstalledAgents();
  const hiddenTargetIds = useMemo(() => new Set(hiddenIds), [hiddenIds]);
  const {
    profiles,
    bridgeStatuses,
    entries,
    errors,
    profileState,
    loading,
    reload,
  } = useAdapterResources();
  const { defaultPools, loading: poolsLoading } = useRoutePoolState({
    profiles,
    detailTarget: null,
  });
  const inspect = useSideSplit<WriteTarget>({ storageKey: ROUTES_INSPECT_WIDTH_KEY });
  const [revealedTokenId, setRevealedTokenId] = useState<string | null>(null);
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
      );
      if (profileState !== 'error') return next;
      return next.map((row) => ({
        ...row,
        token: null,
        maskedToken: null,
        unavailable: true,
      }));
    },
    [bridgeStatuses, defaultPools, errors.bridgeStatuses, profileState, profiles],
  );

  useEffect(() => {
    if (!revealedTokenId) return undefined;
    const timer = window.setTimeout(() => setRevealedTokenId(null), 8_000);
    return () => window.clearTimeout(timer);
  }, [revealedTokenId]);

  const copyToken = (token: string) => {
    void navigator.clipboard.writeText(token).then(
      () => toast({ title: t('routes.tokens.copied'), variant: 'success' }),
      () => toast({ title: t('routes.tokens.copyFailed'), variant: 'danger' }),
    );
  };

  const openWrite = (row: (typeof rows)[number]) => {
    if (!row.profileId) return;
    const profile = profiles.find((item) => item.id === row.profileId);
    if (!profile) return;
    const status = bridgeStatuses[profile.id];
    const graph = buildRouteGraph({
      profile,
      entries,
      siblingProfiles: profiles,
      port: status?.port ?? profile.localPort,
    });
    inspect.open({ profile, graph });
  };

  const writeTarget = inspect.target;
  const pageLoading = loading || poolsLoading;

  const list = (
    <RoutesPane>
      <PageHeader
        title={t('routes.tokens.title')}
        description={t('routes.tokens.description')}
      />
      <div className={pageRhythm.chromeRow}>
        <p className="min-w-0 truncate text-meta text-muted">{
          localEntry.running
            ? t('routes.tokens.scopeNote')
            : localEntry.profileIds.length === 0 && localEntry.hasEnrolledLogins
              ? t('routes.board.entryNeedRoute')
              : localEntry.profileIds.length === 0
                ? t('routes.tokens.scopeNote')
                : t('routes.tokens.entryStoppedHint')
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
            const token = row.token;
            const displayedToken = token
              ? revealedTokenId === row.id ? token : row.maskedToken
              : null;
            const kindColor = agentCssVar(localEndpointBrandAgentId(row.kind));
            return (
              <ListRow key={row.id} className={LIST_ROW_PAD}>
                <ListRowBody
                  main={
                    <>
                      <span className="min-w-0 truncate text-sm font-medium text-primary">
                        {localEndpointKindLabel(row.kind, t)}
                      </span>
                      <span className="font-mono text-meta" style={{ color: kindColor }}>
                        {row.path}
                      </span>
                      <span className="font-mono text-meta text-muted">
                        {row.endpoint ?? t('routes.pendingPort')}
                      </span>
                      {row.unavailable ? (
                        <span className="text-meta text-muted">{t('routes.runtime.unavailable')}</span>
                      ) : displayedToken ? (
                        <span className="min-w-0 truncate font-mono text-meta text-secondary">
                          {displayedToken}
                        </span>
                      ) : (
                        <span className="text-meta text-muted">{t('routes.tokens.noToken')}</span>
                      )}
                    </>
                  }
                  actions={
                    <div className="flex items-center gap-1.5">
                      {token && !row.unavailable ? (
                        <>
                          <Button
                            variant="outline"
                            size="sm"
                            onClick={() => setRevealedTokenId((current) => (
                              current === row.id ? null : row.id
                            ))}
                          >
                            {revealedTokenId === row.id
                              ? t('common.hideSecret')
                              : t('common.showSecret')}
                          </Button>
                          <Button variant="outline" size="sm" onClick={() => copyToken(token)}>
                            {t('routes.tokens.copy')}
                          </Button>
                        </>
                      ) : null}
                      {row.profileId ? (
                        <Button
                          variant="outline"
                          size="sm"
                          disabled={!token || row.unavailable}
                          onClick={() => openWrite(row)}
                        >
                          {t('routes.tokens.writeClient')}
                        </Button>
                      ) : null}
                    </div>
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
      panel={writeTarget ? (
        <WriteClientConfigDialog
          asPanel
          open
          width={inspect.paneWidth}
          onOpenChange={(open) => { if (!open) inspect.close(); }}
          profile={writeTarget.profile}
          rows={writeTarget.graph.rows}
          host={writeTarget.graph.local.host}
          port={writeTarget.graph.local.port ?? null}
          sourceMissing={writeTarget.graph.source.missing}
          listedModels={writeTarget.graph.listedModels}
          contextWindowTokens={writeTarget.graph.contextWindowTokens}
          localToken={bridgeStatuses[writeTarget.profile.id]?.localToken}
          siblingProfiles={profiles}
          hiddenTargetIds={hiddenTargetIds}
          onWritten={() => {
            inspect.close();
            void reload();
          }}
        />
      ) : null}
    >
      {list}
    </WorkbenchSplitPage>
  );
}
