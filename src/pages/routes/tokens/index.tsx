import { useMemo, useState } from 'react';
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
import { Switch } from '@/components/ui/switch';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { useToast } from '@/components/ui/toast';
import { useInstalledAgents } from '@/lib/hooks/useInstalledAgents';
import { ROUTES_POOL_PATH } from '@/lib/bridges-path';
import { setChatCompletionsShared } from '@/lib/api/adapter';
import { localEndpointBrandAgentId } from '@/lib/route-endpoints';
import { agentCssVar } from '@/styles/tokens';
import { WriteClientConfigDialog } from '@/pages/bridges/WriteClientConfigDialog';
import { buildRouteGraph } from '@/pages/bridges/route-graph-model';
import {
  ROUTES_INSPECT_WIDTH_KEY,
  type WriteTarget,
} from '@/pages/bridges/route-inspect';
import { useAdapterResources } from '@/pages/bridges/use-bridge-resources';
import { useRoutePoolState } from '@/pages/bridges/use-route-pool-state';
import { buildLocalEntryControl } from '@/pages/routes/board/board-view-model';
import { RoutesPane } from '@/pages/routes/RoutesPane';
import { TokenDetailPanel } from './TokenDetailPanel';
import { buildLocalTokenRows, tokenRowTitle, type LocalTokenRow } from './tokens-model';

type TokensInspect =
  | { kind: 'detail'; rowId: string }
  | { kind: 'write'; target: WriteTarget };

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
  const [poolTick, setPoolTick] = useState(0);
  const {
    routePoolV2,
    chatCompletionsShared,
    defaultPools,
    loading: poolsLoading,
  } = useRoutePoolState({
    profiles,
    detailTarget: null,
    reloadKey: poolTick,
  });
  const inspect = useSideSplit<TokensInspect>({ storageKey: ROUTES_INSPECT_WIDTH_KEY });
  const [shareBusy, setShareBusy] = useState(false);
  const [shareConfirm, setShareConfirm] = useState<boolean | null>(null);
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

  const openWrite = (row: LocalTokenRow) => {
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
    inspect.open({ kind: 'write', target: { profile, graph } });
  };

  const applyShare = async (shared: boolean) => {
    if (shareBusy) return;
    setShareBusy(true);
    try {
      await setChatCompletionsShared(shared);
      setShareConfirm(null);
      setPoolTick((tick) => tick + 1);
      void reload();
    } catch {
      toast({ title: t('routes.tokens.shareFailed'), variant: 'danger' });
    } finally {
      setShareBusy(false);
    }
  };

  const inspectTarget = inspect.target;
  const detailRow = inspectTarget?.kind === 'detail'
    ? rows.find((row) => row.id === inspectTarget.rowId) ?? null
    : null;
  const writeTarget = inspectTarget?.kind === 'write' ? inspectTarget.target : null;
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

      {routePoolV2 ? (
        <label className="flex min-w-0 items-center justify-between gap-3 rounded-card border border-border px-3 py-2">
          <span className="min-w-0 text-sm text-primary">{t('routes.tokens.shareChat')}</span>
          <Switch
            checked={chatCompletionsShared}
            disabled={shareBusy}
            onCheckedChange={(next) => setShareConfirm(next)}
            aria-label={t('routes.tokens.shareChat')}
          />
        </label>
      ) : null}

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
            const kindColor = agentCssVar(localEndpointBrandAgentId(row.kind));
            const title = tokenRowTitle(row, chatCompletionsShared, t);
            return (
              <ListRow
                key={row.id}
                className={LIST_ROW_PAD}
                active={inspectTarget?.kind === 'detail' && inspectTarget.rowId === row.id}
                onOpen={() => inspect.open({ kind: 'detail', rowId: row.id })}
                aria-label={t('routes.tokens.openDetailAria', { name: title })}
              >
                <ListRowBody
                  main={
                    <>
                      <span className="min-w-0 truncate text-sm font-medium text-primary">
                        {title}
                      </span>
                      <span className="font-mono text-meta" style={{ color: kindColor }}>
                        {row.path}
                      </span>
                      <span className="font-mono text-meta text-muted">
                        {row.endpoint ?? t('routes.pendingPort')}
                      </span>
                      {row.unavailable ? (
                        <span className="text-meta text-muted">{t('routes.runtime.unavailable')}</span>
                      ) : row.maskedToken ? (
                        <span className="min-w-0 truncate font-mono text-meta text-secondary">
                          {row.maskedToken}
                        </span>
                      ) : (
                        <span className="text-meta text-muted">{t('routes.tokens.noToken')}</span>
                      )}
                    </>
                  }
                />
              </ListRow>
            );
          })}
        </div>
      )}

      <Dialog open={shareConfirm != null} onOpenChange={(open) => { if (!open) setShareConfirm(null); }}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t('routes.tokens.shareChat')}</DialogTitle>
            <DialogDescription>
              {shareConfirm
                ? t('routes.tokens.shareChatOn')
                : t('routes.tokens.shareChatOff')}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setShareConfirm(null)} disabled={shareBusy}>
              {t('common.cancel')}
            </Button>
            <Button
              onClick={() => { if (shareConfirm != null) void applyShare(shareConfirm); }}
              disabled={shareBusy || shareConfirm == null}
            >
              {shareConfirm
                ? t('routes.tokens.shareChatConfirmOn')
                : t('routes.tokens.shareChatConfirmOff')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
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
      ) : detailRow ? (
        <TokenDetailPanel
          row={detailRow}
          chatCompletionsShared={chatCompletionsShared}
          width={inspect.paneWidth}
          onWrite={detailRow.profileId ? () => openWrite(detailRow) : undefined}
          onClose={() => inspect.close()}
        />
      ) : null}
    >
      {list}
    </WorkbenchSplitPage>
  );
}
