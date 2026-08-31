import { useEffect, useMemo, useRef, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import { Boxes, Loader2 } from 'lucide-react';
import { useTicketWallet } from '@/app/runtime';
import { PageHeader } from '@/components/layout/PageHeader';
import { PageSection } from '@/components/layout/PageSection';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { WorkbenchSplitPage } from '@/components/layout/SideSplit';
import { useSideSplit } from '@/components/layout/use-side-split';
import { EmptyState } from '@/components/shared/EmptyState';
import { ErrorState } from '@/components/shared/ErrorState';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Notice } from '@/components/shared/Notice';
import { Skeleton } from '@/components/ui/skeleton';
import { Button } from '@/components/ui/button';
import { Tip } from '@/components/ui/tooltip';
import {
  closeConfirmationOnOpenChange,
  preventBusyConfirmationDismissal,
} from '@/components/shared/busy-confirmation';
import { useToast } from '@/components/ui/toast';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { ApiKeyAccountDialog } from '@/components/connections/ApiKeyAccountDialog';
import { ProviderEditDialog } from '@/components/connections/ProviderEditDialog';
import type { AdapterProfile } from '@/lib/backend/contracts/adapter';
import type { TicketView } from '@/lib/api/tickets';
import { deleteAccount } from '@/lib/api/account';
import { removeRouteAuthorization, setRouteAuthorizationEnabled } from '@/lib/api/adapter';
import { deleteProvider } from '@/lib/api/provider';
import { guiErrorCode, logGuiEvent } from '@/lib/api/settings';
import { useInstalledAgents } from '@/lib/hooks/useInstalledAgents';
import { AdapterErrorLines, AdapterProfiles } from '@/pages/bridges/adapter-components';
import {
  resolveBridgesProfileQuery,
  resourceFailureMessage,
} from '@/pages/bridges/adapter-model';
import {
  adapterProfileFlowLabel,
  bridgesPageViewState,
  groupLocalBridgeProfiles,
  partitionLocalBridgeRuntimes,
} from '@/pages/bridges/adapter-view-model';
import { EditRouteDialog } from '@/pages/bridges/EditRouteDialog';
import { RouteDetailPanel } from '@/pages/bridges/RouteDetailPanel';
import { WriteClientConfigDialog } from '@/pages/bridges/WriteClientConfigDialog';
import { buildRouteGraph } from '@/pages/bridges/route-graph-model';
import {
  inspectAuthorizationKey,
  inspectProfileId,
  liveInspectProfile,
  ROUTES_INSPECT_WIDTH_KEY,
  type RouteInspect,
} from '@/pages/bridges/route-inspect';
import {
  collectPoolAuthorizations,
  directProfilesForRoutePoolV2,
  matchDefaultPoolForProfile,
  poolAuthorizationDeleteSteps,
  poolAuthorizationTicketView,
} from '@/pages/bridges/route-pool-view-model';
import { useAdapterResources } from '@/pages/bridges/use-bridge-resources';
import { useBridgeRuntimeActions } from '@/pages/bridges/use-bridge-runtime-actions';
import { useRoutePoolState } from '@/pages/bridges/use-route-pool-state';
import {
  deleteConnectionDialogDescription,
  deleteConnectionToastDescription,
} from '@/pages/connections/connection-model';
import { useOAuthLoginAgents } from '@/pages/connections/use-oauth-login-agents';
import { PoolAddButtons } from './PoolAddButtons';
import { PoolAuthorizationDetail } from './PoolAuthorizationDetail';
import { PoolAuthorizationList } from './PoolAuthorizationList';

/**
 * 连接池：每一份官方登录 / API Key 一行，只展示登录状态。
 * 看板深链仍可在此打开路由详情。
 */
export default function RoutesPoolPage() {
  const { t } = useI18n();
  const { toast } = useToast();
  const [searchParams] = useSearchParams();
  const profileQuery = searchParams.get('profile');
  const openedProfileQueryRef = useRef<string | null>(null);
  const {
    entries,
    profiles,
    bridgeStatuses,
    errors: resourceErrors,
    profileState,
    loading,
    wallet,
    reload,
    reloadProfiles,
    updateBridgeStatus,
    removeProfile,
  } = useAdapterResources();
  const { hiddenIds, installedIds, visibleIds, loading: agentsLoading } = useInstalledAgents();
  const hiddenTargetIds = useMemo(() => new Set(hiddenIds), [hiddenIds]);
  const allowedAgents = installedIds.length > 0 || !agentsLoading ? installedIds : visibleIds;
  const oauthLoginAgents = useOAuthLoginAgents(allowedAgents);
  const ticketWallet = useTicketWallet();
  const inspect = useSideSplit<RouteInspect>({ storageKey: ROUTES_INSPECT_WIDTH_KEY });
  const [poolReloadKey, setPoolReloadKey] = useState(0);
  const [deleteTicket, setDeleteTicket] = useState<TicketView | null>(null);
  const [deleteBusy, setDeleteBusy] = useState(false);
  const [togglingKey, setTogglingKey] = useState<string | null>(null);
  const reloadAll = () => {
    void reload();
    setPoolReloadKey((value) => value + 1);
  };

  const runtime = useBridgeRuntimeActions({
    profiles,
    hiddenTargetIds,
    reloadProfiles,
    updateBridgeStatus,
    removeProfile,
    t,
    toast,
    onEnrollDone: () => inspect.close(),
  });
  const {
    removeConfirm,
    setRemoveConfirm,
    stopConfirm,
    setStopConfirm,
    removingProfileId,
    profileErrors,
    busyProfileIds,
    enrollingProfileId,
    handleStartBridge,
    confirmStopBridge,
    confirmRemove,
    handleEnrollNative,
  } = runtime;

  useEffect(() => {
    if (loading && profiles.length === 0) return;
    if (openedProfileQueryRef.current === profileQuery) return;
    const resolvedId = resolveBridgesProfileQuery(profileQuery, profiles);
    openedProfileQueryRef.current = profileQuery;
    if (!resolvedId) return;
    const profile = profiles.find((row) => row.id === resolvedId);
    if (!profile) return;
    inspect.open({ kind: 'detail', profile });
  }, [inspect.open, loading, profileQuery, profiles]);

  const inspectTarget = inspect.target;
  const detailTarget = inspectTarget?.kind === 'detail'
    ? liveInspectProfile(inspectTarget.profile, profiles)
    : null;

  const { routePoolV2, defaultPools, nativeCanApplyById } = useRoutePoolState({
    profiles,
    detailTarget,
    reloadKey: poolReloadKey,
  });

  const { bound, orphan } = useMemo(
    () => partitionLocalBridgeRuntimes(profiles, {
      entries,
      bindingProfileIds: wallet.bindingProfileIds,
    }),
    [entries, profiles, wallet.bindingProfileIds],
  );
  const groupedOrphan = useMemo(
    () => groupLocalBridgeProfiles(orphan, bridgeStatuses),
    [orphan, bridgeStatuses],
  );
  const bindingCounts = useMemo(() => {
    const counts = new Map<string, number>();
    for (const binding of ticketWallet.wallet?.bindings ?? []) {
      counts.set(binding.ticketId, (counts.get(binding.ticketId) ?? 0) + 1);
    }
    return counts;
  }, [ticketWallet.wallet]);
  const authorizations = useMemo(
    () => collectPoolAuthorizations(
      defaultPools,
      entries,
      bindingCounts,
      t('routes.pool.detail.identityUnavailable'),
    ),
    [bindingCounts, defaultPools, entries, t],
  );
  const authorizationItem = inspectTarget?.kind === 'authorization'
    ? authorizations.find((item) => item.key === inspectTarget.key) ?? null
    : null;
  const authorizationEntry = authorizationItem
    ? entries.find((entry) => entry.key === authorizationItem.key) ?? null
    : null;
  const authorizationTicket = authorizationItem
    ? poolAuthorizationTicketView(
      authorizationItem,
      ticketWallet.wallet?.tickets.find((ticket) => ticket.id === authorizationItem.key),
    )
    : null;

  useEffect(() => {
    if (loading) return;
    if (inspectTarget?.kind !== 'authorization') return;
    if (authorizations.some((item) => item.key === inspectTarget.key)) return;
    inspect.close();
  }, [authorizations, inspect.close, inspectTarget, loading]);
  const directProfiles = useMemo(
    () => directProfilesForRoutePoolV2(routePoolV2, profiles, bridgeStatuses),
    [bridgeStatuses, profiles, routePoolV2],
  );

  const inspectProfile = inspectTarget?.kind === 'edit'
    ? liveInspectProfile(inspectTarget.profile, profiles)
    : inspectTarget?.kind === 'write'
      ? liveInspectProfile(inspectTarget.target.profile, profiles)
      : detailTarget;
  const detailPool = inspectProfile && routePoolV2
    ? matchDefaultPoolForProfile(defaultPools, inspectProfile)
    : null;
  const connectionWarning = resourceFailureMessage(resourceErrors);
  const stopError = stopConfirm ? profileErrors[stopConfirm.id] : null;
  const removeError = removeConfirm ? profileErrors[removeConfirm.id] : null;
  const stopDialogBusy = Boolean(stopConfirm && busyProfileIds[stopConfirm.id]);
  const removeDialogBusy = removingProfileId !== null;
  const removeConfirmIsOrphan = Boolean(
    removeConfirm && orphan.some((profile) => profile.id === removeConfirm.id),
  );
  const orphanOnly = groupedOrphan.length > 0 && authorizations.length === 0;
  const hasContent = groupedOrphan.length > 0
    || directProfiles.length > 0
    || authorizations.length > 0;
  const pageView = bridgesPageViewState({
    profileState: loading && profileState !== 'error' ? 'loading' : profileState,
    bound,
    orphan: groupedOrphan,
    wallet: {
      settled: wallet.settled,
      lastWalletBridgeCount: wallet.lastWalletBridgeCount,
    },
  });
  const writeTarget = inspectTarget?.kind === 'write' ? inspectTarget.target : null;
  const editTarget = inspectTarget?.kind === 'edit'
    ? liveInspectProfile(inspectTarget.profile, profiles)
    : null;
  const activeProfileId = inspectProfileId(inspectTarget);

  const listProps = {
    bridgeStatuses,
    statusErrors: resourceErrors.bridgeStatuses,
    entries,
    errors: profileErrors,
    busyProfileIds,
    removingProfileId,
    onStartBridge: handleStartBridge,
    onRequestStopBridge: setStopConfirm,
    onRequestWrite: (profile: AdapterProfile, graph: ReturnType<typeof buildRouteGraph>) => {
      inspect.open({ kind: 'write', target: { profile, graph } });
    },
    onRequestEdit: (profile: AdapterProfile) => {
      inspect.open({ kind: 'edit', profile });
    },
    onShowDetail: (profile: AdapterProfile) => {
      inspect.open({ kind: 'detail', profile });
    },
    activeProfileId,
    onRetry: reloadAll,
    hiddenTargetIds,
    siblingProfiles: profiles,
  };

  const handleAuthorizationEnabled = async (item: typeof authorizations[number], enabled: boolean) => {
    setTogglingKey(item.key);
    try {
      await setRouteAuthorizationEnabled(item.sourceKind, item.sourceId, enabled);
      reloadAll();
    } catch (error) {
      toast({
        title: t('routes.pool.detail.toggleFailed'),
        description: error instanceof Error ? error.message : String(error),
        variant: 'danger',
      });
    } finally {
      setTogglingKey(null);
    }
  };

  const openAuthorizationEdit = () => {
    if (authorizationEntry?.provider) {
      inspect.open({
        kind: 'provider',
        mode: 'edit',
        agentId: authorizationEntry.provider.agentId,
        provider: authorizationEntry.provider,
      });
      return;
    }
    if (authorizationEntry?.account?.kind === 'apikey') {
      inspect.open({
        kind: 'account',
        agentId: authorizationEntry.account.agentId,
        account: authorizationEntry.account,
      });
    }
  };

  const confirmDeleteAuthorization = async () => {
    if (!deleteTicket) return;
    setDeleteBusy(true);
    try {
      const sourceMissing = !entries.some((entry) => entry.key === deleteTicket.id);
      for (const step of poolAuthorizationDeleteSteps({ routePoolV2, sourceMissing })) {
        if (step === 'removeMembership') {
          await removeRouteAuthorization(deleteTicket.sourceKind, deleteTicket.sourceId);
          continue;
        }
        if (deleteTicket.sourceKind === 'account') {
          await deleteAccount(deleteTicket.agentId, deleteTicket.sourceId);
        } else {
          await deleteProvider(deleteTicket.agentId, deleteTicket.sourceId);
        }
      }
      void logGuiEvent('delete_connection', { agent: deleteTicket.agentId });
      const deletedCurrent = entries.some((entry) => (
        entry.key === deleteTicket.id && entry.isCurrent
      ));
      setDeleteTicket(null);
      toast({
        title: t(sourceMissing ? 'connections.delete.toastMissing' : 'connections.delete.toastOk'),
        description: sourceMissing
          ? t('connections.delete.toastMissingDescription')
          : deleteConnectionToastDescription({ isCurrent: deletedCurrent }, t),
        variant: 'success',
      });
      reloadAll();
    } catch (error) {
      void logGuiEvent('delete_connection_fail', {
        agent: deleteTicket.agentId,
        code: guiErrorCode(error),
      });
      toast({
        title: t('connections.delete.toastFail'),
        description: error instanceof Error ? error.message : String(error),
        variant: 'danger',
      });
    } finally {
      setDeleteBusy(false);
    }
  };

  const inspectPanel =
    inspectTarget?.kind === 'authorization' && authorizationItem ? (
      <PoolAuthorizationDetail
        item={authorizationItem}
        width={inspect.paneWidth}
        toggling={togglingKey === authorizationItem.key}
        onEnabledChange={(enabled) => {
          void handleAuthorizationEnabled(authorizationItem, enabled);
        }}
        onDelete={() => {
          if (authorizationTicket) setDeleteTicket(authorizationTicket);
        }}
        onEdit={
          authorizationEntry?.provider || authorizationEntry?.account?.kind === 'apikey'
            ? openAuthorizationEdit
            : undefined
        }
        onClose={() => inspect.close()}
      />
    ) : inspectTarget?.kind === 'account' ? (
      <ApiKeyAccountDialog
        asPanel
        open
        width={inspect.paneWidth}
        agentId={inspectTarget.agentId}
        mode={inspectTarget.account ? 'edit' : 'add'}
        account={inspectTarget.account}
        onOpenChange={(open) => { if (!open) inspect.close(); }}
        onSaved={() => {
          inspect.close();
          reloadAll();
        }}
      />
    ) : inspectTarget?.kind === 'provider' ? (
      <ProviderEditDialog
        asPanel
        open
        width={inspect.paneWidth}
        agentId={inspectTarget.agentId}
        mode={inspectTarget.mode}
        provider={inspectTarget.provider}
        onOpenChange={(open) => { if (!open) inspect.close(); }}
        onSaved={() => {
          inspect.close();
          reloadAll();
        }}
      />
    ) : inspectTarget?.kind === 'write' && writeTarget ? (
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
        onWritten={reloadAll}
      />
    ) : inspectTarget?.kind === 'edit' && editTarget ? (
      <EditRouteDialog
        asPanel
        open
        width={inspect.paneWidth}
        onOpenChange={(open) => { if (!open) inspect.close(); }}
        profile={editTarget}
        entries={entries}
        busy={busyProfileIds[editTarget.id] === true}
        onSaved={reloadAll}
        onRequestDelete={setRemoveConfirm}
      />
    ) : inspectTarget?.kind === 'detail' && detailTarget ? (
      <RouteDetailPanel
        id={`route-detail-${detailTarget.id}`}
        asPanel
        open
        width={inspect.paneWidth}
        onOpenChange={(open) => { if (!open) inspect.close(); }}
        profile={detailTarget}
        bridgeStatus={detailTarget.route === 'local_bridge' ? bridgeStatuses[detailTarget.id] : undefined}
        entries={entries}
        siblingProfiles={profiles}
        busy={busyProfileIds[detailTarget.id] === true || removingProfileId === detailTarget.id}
        error={profileErrors[detailTarget.id]}
        onRequestRemove={setRemoveConfirm}
        onRequestEdit={(profile) => inspect.open({ kind: 'edit', profile })}
        targetHidden={hiddenTargetIds.has(detailTarget.targetAgentId)}
        routePoolV2={routePoolV2}
        defaultPool={detailPool}
        canApplyLocalBridge={nativeCanApplyById[detailTarget.id] === true}
        onEnrollNative={handleEnrollNative}
        enrolling={enrollingProfileId === detailTarget.id}
      />
    ) : null;

  return (
    <>
      <WorkbenchSplitPage
        split={inspect}
        resizeAria={t('common.resizeSidePanel')}
        panel={inspectPanel}
      >
        <PageHeader
          title={t('routes.pool.page.title')}
          description={t('routes.pool.page.description')}
        />
        <div className={pageRhythm.chromeRow}>
          {connectionWarning ? (
            <Notice tone="warning" className="min-w-0 flex-1 items-center py-1">
              {connectionWarning}
            </Notice>
          ) : orphanOnly ? (
            <Tip className="min-w-0 truncate text-meta text-secondary" label={t('routes.orphan.description')}>
              {t('routes.orphan.title')}
            </Tip>
          ) : (
            <p className="min-w-0 truncate text-meta text-muted">
              {t('routes.pool.page.chromeHint')}
            </p>
          )}
          <div className={pageRhythm.chromeActions}>
            <PoolAddButtons
              agents={allowedAgents}
              oauthAgents={oauthLoginAgents}
              entries={entries}
              defaultPools={defaultPools}
              onChanged={reloadAll}
            />
            <Button
              type="button"
              size="sm"
              variant="secondary"
              disabled={loading}
              onClick={reloadAll}
            >
              {loading ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : null}
              {t('routes.board.refresh')}
            </Button>
          </div>
        </div>

        <div className={pageRhythm.stackDense}>
          {pageView === 'loading' ? (
            <div className="space-y-2" aria-live="polite">
              <Skeleton className="h-16 w-full" />
              <Skeleton className="h-16 w-full" />
            </div>
          ) : null}
          {pageView === 'list_error' ? (
            <ErrorState
              title={t('routes.loadError')}
              error={resourceErrors.profiles ?? new Error(t('routes.loadError'))}
              onRetry={reloadAll}
            />
          ) : null}
          {pageView === 'wallet_without_runtime' && !hasContent ? (
            <ErrorState
              title={t('routes.walletWithoutRuntime.title')}
              error={new Error(t('routes.walletWithoutRuntime.description'))}
              onRetry={reloadAll}
            />
          ) : null}
          {pageView === 'healthy_empty' && !hasContent ? (
            <EmptyState
              icon={Boxes}
              title={t('routes.pool.page.emptyTitle')}
              description={t('routes.pool.page.emptyDescription')}
            />
          ) : null}
          {pageView === 'list' || (hasContent && pageView !== 'loading' && pageView !== 'list_error') ? (
            <>
              {authorizations.length > 0 ? (
                <PageSection first>
                  <PoolAuthorizationList
                    items={authorizations}
                    activeKey={inspectAuthorizationKey(inspectTarget)}
                    togglingKey={togglingKey}
                    onShowDetail={(item) => inspect.open({ kind: 'authorization', key: item.key })}
                    onEnabledChange={(item, enabled) => {
                      void handleAuthorizationEnabled(item, enabled);
                    }}
                  />
                </PageSection>
              ) : null}
              {groupedOrphan.length > 0 ? (
                <PageSection
                  first={orphanOnly}
                  title={orphanOnly ? undefined : t('routes.orphan.title')}
                  description={orphanOnly ? undefined : t('routes.orphan.description')}
                >
                  <AdapterProfiles
                    {...listProps}
                    profiles={groupedOrphan}
                    loading={false}
                    loadError={null}
                  />
                </PageSection>
              ) : null}
              {directProfiles.length > 0 ? (
                <PageSection
                  first={authorizations.length === 0 && groupedOrphan.length === 0}
                  title={t('routes.direct.title')}
                  description={t('routes.direct.description')}
                >
                  <AdapterProfiles
                    {...listProps}
                    onRequestWrite={undefined}
                    onRequestEdit={undefined}
                    profiles={directProfiles}
                    loading={false}
                    loadError={null}
                  />
                </PageSection>
              ) : null}
            </>
          ) : null}
        </div>
      </WorkbenchSplitPage>

      <Dialog
        open={Boolean(deleteTicket)}
        onOpenChange={(open) => {
          if (!open && !deleteBusy) setDeleteTicket(null);
        }}
      >
        <DialogContent
          className="max-w-sm"
          hideClose={deleteBusy}
          onEscapeKeyDown={(event) => preventBusyConfirmationDismissal(deleteBusy, event)}
          onPointerDownOutside={(event) => preventBusyConfirmationDismissal(deleteBusy, event)}
          onInteractOutside={(event) => preventBusyConfirmationDismissal(deleteBusy, event)}
        >
          <DialogHeader>
            <DialogTitle>{t('connections.delete.title')}</DialogTitle>
            <DialogDescription>
              {deleteTicket
                ? `${deleteTicket.label} · ${!entries.some((entry) => entry.key === deleteTicket.id)
                  ? t('connections.delete.dialogMissing')
                  : deleteConnectionDialogDescription({
                    isCurrent: entries.some((entry) => entry.key === deleteTicket.id && entry.isCurrent),
                  }, t)}`
                : ''}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="secondary" disabled={deleteBusy} onClick={() => setDeleteTicket(null)}>
              {t('common.cancel')}
            </Button>
            <Button
              variant="danger"
              disabled={deleteBusy}
              onClick={() => void confirmDeleteAuthorization()}
            >
              {deleteBusy
                ? t('connections.delete.deleting')
                : t(!entries.some((entry) => entry.key === deleteTicket?.id)
                  ? 'connections.delete.confirmMissing'
                  : 'connections.delete.confirm')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog
        open={Boolean(stopConfirm)}
        onOpenChange={(open) => closeConfirmationOnOpenChange(open, stopDialogBusy, () => setStopConfirm(null))}
      >
        <DialogContent
          className="flex max-h-[calc(100vh-2rem)] flex-col overflow-hidden"
          hideClose={stopDialogBusy}
          onEscapeKeyDown={(event) => preventBusyConfirmationDismissal(stopDialogBusy, event)}
          onPointerDownOutside={(event) => preventBusyConfirmationDismissal(stopDialogBusy, event)}
          onInteractOutside={(event) => preventBusyConfirmationDismissal(stopDialogBusy, event)}
        >
          <DialogHeader className="shrink-0">
            <DialogTitle>{t('routes.stop.title')}</DialogTitle>
            <DialogDescription>{t('routes.stop.description')}</DialogDescription>
          </DialogHeader>
          <div className="min-h-0 flex-1 space-y-3 overflow-y-auto pr-1">
            {stopConfirm && <p className="text-sm text-secondary">{adapterProfileFlowLabel(stopConfirm, entries)}</p>}
            {stopError ? <AdapterErrorLines error={stopError} fallback={t('routes.stop.fallback')} /> : null}
          </div>
          <DialogFooter className="mt-4 shrink-0 border-t border-border pt-4">
            <Button variant="secondary" onClick={() => setStopConfirm(null)} disabled={stopDialogBusy}>{t('common.cancel')}</Button>
            <Button variant="danger" onClick={() => void confirmStopBridge()} disabled={stopDialogBusy}>
              {stopDialogBusy ? t('routes.stop.confirming') : t('routes.stop.confirm')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog
        open={Boolean(removeConfirm)}
        onOpenChange={(open) => closeConfirmationOnOpenChange(open, removeDialogBusy, () => setRemoveConfirm(null))}
      >
        <DialogContent
          className="flex max-h-[calc(100vh-2rem)] flex-col overflow-hidden"
          hideClose={removeDialogBusy}
          onEscapeKeyDown={(event) => preventBusyConfirmationDismissal(removeDialogBusy, event)}
          onPointerDownOutside={(event) => preventBusyConfirmationDismissal(removeDialogBusy, event)}
          onInteractOutside={(event) => preventBusyConfirmationDismissal(removeDialogBusy, event)}
        >
          <DialogHeader className="shrink-0">
            <DialogTitle>{t('routes.unbind.title')}</DialogTitle>
            <DialogDescription>
              {t(removeConfirmIsOrphan ? 'routes.unbind.orphanDescription' : 'routes.unbind.description')}
            </DialogDescription>
          </DialogHeader>
          <div className="min-h-0 flex-1 space-y-3 overflow-y-auto pr-1">
            {removeConfirm && <p className="text-sm text-secondary">{adapterProfileFlowLabel(removeConfirm, entries)}</p>}
            {removeError ? <AdapterErrorLines error={removeError} fallback={t('routes.unbind.fallback')} /> : null}
          </div>
          <DialogFooter className="mt-4 shrink-0 border-t border-border pt-4">
            <Button variant="secondary" onClick={() => setRemoveConfirm(null)} disabled={removeDialogBusy}>{t('common.cancel')}</Button>
            <Button variant="danger" onClick={() => void confirmRemove()} disabled={removeDialogBusy}>
              {removeDialogBusy ? t('routes.unbind.confirming') : removeError ? t('routes.unbind.retry') : t('routes.unbind.confirm')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
