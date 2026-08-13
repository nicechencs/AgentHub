import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Link, useNavigate, useSearchParams } from 'react-router-dom';
import { Boxes, ChevronRight } from 'lucide-react';
import { PageHeader } from '@/components/layout/PageHeader';
import { PageSection } from '@/components/layout/PageSection';
import { EmptyState } from '@/components/shared/EmptyState';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { kindBadge } from '@/pages/connections/connection-model';
import { useAgentStatusesOptional } from '@/app/runtime';
import { AGENT_IDS } from '@/config/agents';
import {
  applyAdapter,
  getAdapterBridgeStatus,
  planAdapter,
  removeAdapter,
  setAdapterBridgeAutoStart,
  startAdapterBridge,
  stopAdapterBridge,
} from '@/lib/api/adapter';
import type {
  AdapterApplyPlan,
  AdapterBridgeRuntimeStatus,
  AdapterProfile,
} from '@/lib/backend/contracts/adapter';
import type { AgentId } from '@/lib/types';
import {
  AdapterErrorLines,
  AdapterPreviewList,
  AdapterPreviewResult,
  AdapterProfiles,
} from './adapter-components';
import { AdapterSourceList } from './AdapterSourceList';
import {
  adapterApplyCommit,
  adapterPageDescription,
  adapterPageViewState,
  adapterProfileRecordLabel,
  canApplyAdapterPlan,
  canRequestAdapterPlan,
  isCurrentAdapterPreviewRequest,
  parseAdapterCredentialFilter,
  resolveAdapterTargetAgentId,
  resourceFailureMessage,
  sourceLabel,
  sourceStatusHint,
  targetAgentName,
  type AdapterCredentialFilter,
} from './adapter-model';
import {
  adapterApplyStage,
  adapterApplyStageLabel,
  adapterBridgeProbeSummary,
  adapterSourceCounts,
  excludeAdapterGeneratedSources,
  filterAdapterSourcesByCredential,
  groupAdapterSources,
  isOAuthAuthIncomplete,
  oauthIncompleteAuthHint,
  searchAdapterSources,
  selectableTargetAgentIds,
} from './adapter-sources';
import { useAdapterResources } from './use-adapter-resources';
import {
  closeConfirmationOnOpenChange,
  preventBusyConfirmationDismissal,
} from '@/components/shared/busy-confirmation';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';

export {
  adapterActionLabel,
  adapterApplyCommit,
  adapterBridgeEndpointLabel,
  adapterBridgeStateLabel,
  adapterBridgeUpstreamLabel,
  adapterCredentialFilterLabel,
  adapterCredentialKindLabel,
  adapterPageDescription,
  adapterPageViewState,
  adapterPlanChangeLabel,
  adapterProfileRecordLabel,
  adapterProfileStatusLabel,
  adapterTabDescription,
  adapterTabLabel,
  adapterTableRouteLabel,
  canApplyAdapterPlan,
  canRequestAdapterPlan,
  connectionKindForFilter,
  connectionKindForTab,
  filterProfilesByCredential,
  filterProfilesByMode,
  futureAvailability,
  isCurrentAdapterPreviewRequest,
  isSubscriptionGateUnsupported,
  maskedIdSuffix,
  parseAdapterCredentialFilter,
  parseAdapterTab,
  resolveAdapterTargetAgentId,
  routeLabel,
  sourceLabel,
  sourceStatusHint,
  unsupportedPresentation,
} from './adapter-model';

export {
  closeConfirmationOnOpenChange,
  preventBusyConfirmationDismissal,
} from '@/components/shared/busy-confirmation';

function connectionsHref(filter: AdapterCredentialFilter): string {
  if (filter === 'oauth') return '/connections?mode=oauth';
  if (filter === 'api') return '/connections?mode=api';
  return '/connections';
}

/** Adapter compatibility preview and saved generated projections. */
export default function AdapterPage() {
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();
  const filter = parseAdapterCredentialFilter(searchParams.get('tab'));
  const {
    entries,
    profiles,
    bridgeStatuses,
    errors: resourceErrors,
    connectionState,
    profileState,
    loading,
    reload,
    reloadProfiles,
    updateBridgeStatus,
    updateProfile,
    removeProfile,
  } = useAdapterResources();
  const agentStatusSnapshot = useAgentStatusesOptional();
  const [sourceKey, setSourceKey] = useState('');
  const [sourceQuery, setSourceQuery] = useState('');
  const [targetAgentId, setTargetAgentId] = useState<AgentId | ''>('');
  const [plan, setPlan] = useState<AdapterApplyPlan | null>(null);
  const [analyzing, setAnalyzing] = useState(false);
  const [analysisError, setAnalysisError] = useState<unknown>(null);
  const [applyConfirmOpen, setApplyConfirmOpen] = useState(false);
  const [applying, setApplying] = useState(false);
  const [applyError, setApplyError] = useState<unknown>(null);
  const [applySuccess, setApplySuccess] = useState<string | null>(null);
  const [applyResultProfile, setApplyResultProfile] = useState<AdapterProfile | null>(null);
  const [applyProbeStatus, setApplyProbeStatus] = useState<AdapterBridgeRuntimeStatus | null>(null);
  const [removeConfirm, setRemoveConfirm] = useState<AdapterProfile | null>(null);
  const [stopConfirm, setStopConfirm] = useState<AdapterProfile | null>(null);
  const [removingProfileId, setRemovingProfileId] = useState<string | null>(null);
  const [profileErrors, setProfileErrors] = useState<Record<string, unknown>>({});
  const [busyProfileIds, setBusyProfileIds] = useState<Record<string, boolean>>({});
  const [retryToken, setRetryToken] = useState(0);
  const requestGeneration = useRef(0);

  const targetAgentIds = useMemo(
    () => selectableTargetAgentIds({
      state: agentStatusSnapshot.state,
      statuses: agentStatusSnapshot.statuses,
      fallbackIds: AGENT_IDS,
    }),
    [agentStatusSnapshot.state, agentStatusSnapshot.statuses],
  );
  const allSelectableEntries = useMemo(
    () => excludeAdapterGeneratedSources(entries, profiles),
    [entries, profiles],
  );
  const sourceCounts = useMemo(
    () => adapterSourceCounts(allSelectableEntries),
    [allSelectableEntries],
  );
  const filteredEntries = useMemo(
    () => searchAdapterSources(
      filterAdapterSourcesByCredential(allSelectableEntries, filter),
      sourceQuery,
    ),
    [allSelectableEntries, filter, sourceQuery],
  );
  const sourceGroups = useMemo(
    () => groupAdapterSources(filteredEntries, AGENT_IDS),
    [filteredEntries],
  );

  const setFilter = (next: AdapterCredentialFilter) => {
    const params = new URLSearchParams(searchParams);
    if (next === 'all') params.delete('tab');
    else params.set('tab', next);
    setSearchParams(params, { replace: true });
  };

  const source = useMemo(
    () => allSelectableEntries.find((entry) => entry.key === sourceKey) ?? null,
    [allSelectableEntries, sourceKey],
  );
  const sourceAuthIncomplete = isOAuthAuthIncomplete(source);
  const sourceBadge = source ? kindBadge(source.kind) : null;

  const resolvedTargetAgentId = resolveAdapterTargetAgentId(targetAgentId, targetAgentIds);

  useEffect(() => {
    if (resolvedTargetAgentId === targetAgentId) return;
    setTargetAgentId(resolvedTargetAgentId);
  }, [resolvedTargetAgentId, targetAgentId]);

  useEffect(() => {
    if (sourceKey && !allSelectableEntries.some((entry) => entry.key === sourceKey)) {
      setSourceKey('');
    }
  }, [allSelectableEntries, sourceKey]);
  const visibleProfileErrors = { ...resourceErrors.bridgeStatuses, ...profileErrors };

  // Every selection (and retry) starts a plan request. The generation
  // check prevents an old result from replacing the visible selection.
  useEffect(() => {
    const generation = ++requestGeneration.current;
    setPlan(null);
    setAnalysisError(null);
    // A previous apply failure or success must not stick on a newly selected source/target.
    setApplyError(null);
    setApplySuccess(null);
    setApplyResultProfile(null);
    setApplyProbeStatus(null);
    if (!source || !resolvedTargetAgentId || !canRequestAdapterPlan({
      sourceId: source.id,
      targetAgentId: resolvedTargetAgentId,
    })) {
      setAnalyzing(false);
      return;
    }
    const request = {
      sourceKind: source.source,
      sourceId: source.id,
      targetAgentId: resolvedTargetAgentId,
    } as const;
    setAnalyzing(true);
    void planAdapter(request)
      .then((nextPlan) => {
        if (!isCurrentAdapterPreviewRequest(generation, requestGeneration.current)) return;
        setPlan(nextPlan);
      })
      .catch((error) => {
        if (isCurrentAdapterPreviewRequest(generation, requestGeneration.current)) setAnalysisError(error);
      })
      .finally(() => {
        if (isCurrentAdapterPreviewRequest(generation, requestGeneration.current)) setAnalyzing(false);
      });
  }, [resolvedTargetAgentId, retryToken, source]);

  const preview = plan?.analysis ?? null;
  const retryPreview = () => setRetryToken((token) => token + 1);
  const canApply = Boolean(resolvedTargetAgentId) && canApplyAdapterPlan(plan) && !sourceAuthIncomplete;
  const applyRequest = source && resolvedTargetAgentId ? {
    sourceKind: source.source,
    sourceId: source.id,
    targetAgentId: resolvedTargetAgentId,
  } as const : null;
  const applyStage = adapterApplyStage({
    applying,
    successMessage: applySuccess,
    error: applyError,
    profileStatus: applyResultProfile?.status,
  });
  const applyStageText = adapterApplyStageLabel(applyStage);
  const applyProbeText = applyResultProfile?.route === 'local_bridge'
    ? adapterBridgeProbeSummary(applyProbeStatus ?? (applyResultProfile ? bridgeStatuses[applyResultProfile.id] : undefined))
    : null;

  const setProfileBusy = (profileId: string, busy: boolean) => {
    setBusyProfileIds((current) => ({ ...current, [profileId]: busy }));
  };

  const clearProfileError = (profileId: string) => {
    setProfileErrors((current) => {
      const { [profileId]: _ignored, ...remaining } = current;
      return remaining;
    });
  };

  const reloadThenClearProfileErrors = () => {
    // apply/remove already notify the shared pool; only profiles need a second pass.
    void reloadProfiles().then(
      () => { setProfileErrors({}); },
      () => undefined,
    );
  };

  const setBridgeStatusBestEffort = useCallback(async (profile: AdapterProfile) => {
    try {
      const status = await getAdapterBridgeStatus(profile.id);
      updateBridgeStatus(status);
      setApplyProbeStatus(status);
      return status;
    } catch (error) {
      setProfileErrors((current) => ({
        ...current,
        [profile.id]: error,
      }));
      return null;
    }
  }, [updateBridgeStatus]);

  const confirmApply = async () => {
    if (!applyRequest || !canApply) return;
    setApplying(true);
    setApplyError(null);
    setApplySuccess(null);
    setApplyResultProfile(null);
    setApplyProbeStatus(null);
    try {
      const result = await applyAdapter(applyRequest);
      const committed = adapterApplyCommit(result);
      // Applying is the committed operation. Close the confirmation and show success
      // before any optional runtime inspection can fail or block the refresh.
      setApplyResultProfile(result.profile);
      setApplySuccess(committed.successMessage);
      setApplyConfirmOpen(false);
      if (committed.shouldProbeBridge) void setBridgeStatusBestEffort(result.profile);
      if (committed.shouldRefresh) reloadThenClearProfileErrors();
    } catch (error) {
      setApplyError(error);
    } finally {
      setApplying(false);
    }
  };

  const handleStartBridge = async (profile: AdapterProfile) => {
    setProfileBusy(profile.id, true);
    clearProfileError(profile.id);
    try {
      updateBridgeStatus(await startAdapterBridge(profile.id));
      reloadThenClearProfileErrors();
    } catch (error) {
      setProfileErrors((current) => ({ ...current, [profile.id]: error }));
    } finally {
      setProfileBusy(profile.id, false);
    }
  };

  const confirmStopBridge = async () => {
    if (!stopConfirm) return;
    const profile = stopConfirm;
    setProfileBusy(profile.id, true);
    clearProfileError(profile.id);
    try {
      updateBridgeStatus(await stopAdapterBridge(profile.id));
      setStopConfirm(null);
      reloadThenClearProfileErrors();
    } catch (error) {
      setProfileErrors((current) => ({ ...current, [profile.id]: error }));
    } finally {
      setProfileBusy(profile.id, false);
    }
  };

  const handleSetBridgeAutoStart = async (profile: AdapterProfile, autoStart: boolean) => {
    setProfileBusy(profile.id, true);
    clearProfileError(profile.id);
    try {
      updateProfile(await setAdapterBridgeAutoStart(profile.id, autoStart));
      reloadThenClearProfileErrors();
    } catch (error) {
      setProfileErrors((current) => ({ ...current, [profile.id]: error }));
    } finally {
      setProfileBusy(profile.id, false);
    }
  };

  const confirmRemove = async () => {
    if (!removeConfirm) return;
    const profileId = removeConfirm.id;
    setRemovingProfileId(profileId);
    clearProfileError(profileId);
    try {
      await removeAdapter(profileId);
      removeProfile(profileId);
      setRemoveConfirm(null);
      reloadThenClearProfileErrors();
    } catch (error) {
      setProfileErrors((errors) => ({ ...errors, [profileId]: error }));
    } finally {
      setRemovingProfileId(null);
    }
  };

  const connectionLoadError = connectionState === 'error'
    ? resourceErrors.accounts ?? resourceErrors.providers
    : null;
  const viewState = adapterPageViewState({
    loading: loading && entries.length === 0,
    loadError: connectionLoadError,
    entriesCount: allSelectableEntries.length,
    hasSource: Boolean(source),
  });
  const connectionWarning = resourceFailureMessage(resourceErrors);
  const stopError = stopConfirm ? profileErrors[stopConfirm.id] : null;
  const removeError = removeConfirm ? profileErrors[removeConfirm.id] : null;
  const stopDialogBusy = Boolean(stopConfirm && busyProfileIds[stopConfirm.id]);
  const removeDialogBusy = removingProfileId !== null;

  return (
    <div>
      <PageHeader
        title="Adapter"
        badge={<Badge variant="warning">开发中</Badge>}
        description={adapterPageDescription()}
        descriptionTip="不会把一家 OAuth 凭据“转换”为另一家授权，也不会在日志记录请求正文。桥接仅监听本机 127.0.0.1。"
        actions={(
          <Button variant="outline" onClick={() => navigate(connectionsHref(filter))}>
            去 Connections
          </Button>
        )}
      />

      {connectionWarning && <p className="mb-3 text-sm text-warning" role="alert">{connectionWarning}</p>}

      <div className="grid grid-cols-1 gap-4 lg:grid-cols-[minmax(16rem,20rem)_minmax(0,1fr)] lg:items-start">
        <Card className="flex min-h-[24rem] flex-col overflow-hidden lg:sticky lg:top-6 lg:max-h-[calc(100vh-8rem)]">
          <CardContent className="flex min-h-0 flex-1 flex-col p-4">
            <AdapterSourceList
              groups={sourceGroups}
              selectedKey={sourceKey}
              filter={filter}
              counts={sourceCounts}
              query={sourceQuery}
              loading={viewState === 'loading'}
              loadError={viewState === 'error' ? connectionLoadError : null}
              totalCount={allSelectableEntries.length}
              visibleCount={filteredEntries.length}
              onSelect={(entry) => setSourceKey(entry.key)}
              onFilterChange={setFilter}
              onQueryChange={setSourceQuery}
              onRetry={() => void reload()}
              onGoConnections={(next) => navigate(connectionsHref(next))}
            />
          </CardContent>
        </Card>

        <Card className="min-h-[24rem] lg:min-h-[32rem]">
          <CardHeader>
            <div className="min-w-0">
              <CardTitle>路由适配</CardTitle>
              <p className="mt-1 text-sm text-secondary">
                {source
                  ? <>{sourceLabel(source)} <ChevronRight className="inline h-3.5 w-3.5" /> {resolvedTargetAgentId ? targetAgentName(resolvedTargetAgentId) : '未选择目标'}</>
                  : '选择左侧连接后，在此选择目标并预览路径。'}
              </p>
            </div>
          </CardHeader>
          <CardContent className="space-y-4">
            {!source ? (
              <EmptyState
                icon={Boxes}
                title="选择一个来源连接"
                description="左侧同时列出 API Key 与官方登录。选择后会按该连接自己的规则分析路径；未开门禁的组合只展示原因，不会应用。"
              />
            ) : (
              <>
                <section className="space-y-2 rounded-btn border border-border bg-subtle p-3">
                  <div className="flex flex-wrap items-center gap-1.5">
                    <h3 className="text-sm font-medium">来源摘要</h3>
                    {sourceBadge ? <Badge variant={sourceBadge.variant}>{sourceBadge.label}</Badge> : null}
                  </div>
                  <p className="text-sm">{sourceLabel(source)}</p>
                  <p className="text-xs text-muted">{sourceStatusHint(source)}</p>
                  {sourceAuthIncomplete && (
                    <p className="text-sm text-warning" role="status">
                      {oauthIncompleteAuthHint()}{' '}
                      <Link className="underline" to="/connections">前往 Connections</Link>
                    </p>
                  )}
                </section>

                <label className="block text-sm font-medium">
                  目标 Agent
                  <select
                    aria-label="目标 Agent"
                    className="mt-1 w-full rounded-btn border border-border bg-panel px-3 py-2 text-sm"
                    value={resolvedTargetAgentId}
                    onChange={(event) => setTargetAgentId(event.target.value as AgentId)}
                    disabled={targetAgentIds.length === 0}
                  >
                    {targetAgentIds.map((agentId) => (
                      <option key={agentId} value={agentId}>{targetAgentName(agentId)}</option>
                    ))}
                  </select>
                </label>
                {targetAgentIds.length === 0 && (
                  <p className="text-xs text-secondary">当前没有已安装或可配置的目标 Agent。</p>
                )}

                <AdapterPreviewResult
                  analysis={preview}
                  plan={plan}
                  loading={analyzing}
                  error={analysisError}
                  onRetry={retryPreview}
                  onApply={canApply ? () => setApplyConfirmOpen(true) : undefined}
                  applyError={applyError}
                  authIncomplete={sourceAuthIncomplete}
                  authHint={sourceAuthIncomplete ? oauthIncompleteAuthHint() : undefined}
                />

                {(applySuccess || applyError || applying) && (
                  <div className="space-y-1" role={applyError ? 'alert' : 'status'}>
                    {applyStageText && (
                      <p className={`text-sm ${applyError ? 'text-danger' : 'text-success'}`}>
                        {applyStageText}
                        {applySuccess ? ` · ${applySuccess}` : ''}
                      </p>
                    )}
                    {applyProbeText && <p className="text-sm text-secondary">{applyProbeText}</p>}
                    {applyError ? <AdapterErrorLines error={applyError} fallback="应用适配失败" /> : null}
                    {applySuccess && (
                      <p className="text-sm text-success">
                        <Link className="underline" to="/connections">在 Connections 查看</Link>
                      </p>
                    )}
                  </div>
                )}
              </>
            )}
          </CardContent>
        </Card>
      </div>

      <PageSection
        title="已创建的适配"
        description="凭据类型与适配路径分列展示。本地协议转换不是 OAuth。"
        ruled
      >
        <AdapterProfiles
          profiles={profiles}
          bridgeStatuses={bridgeStatuses}
          loading={loading && profiles.length === 0}
          loadError={profileState === 'error' ? resourceErrors.profiles : null}
          errors={visibleProfileErrors}
          removingProfileId={removingProfileId}
          busyProfileIds={busyProfileIds}
          onRemove={setRemoveConfirm}
          onStartBridge={handleStartBridge}
          onRequestStopBridge={setStopConfirm}
          onSetBridgeAutoStart={handleSetBridgeAutoStart}
          onRetry={() => void reload()}
        />
      </PageSection>

      <Dialog
        open={applyConfirmOpen}
        onOpenChange={(open) => closeConfirmationOnOpenChange(open, applying, () => setApplyConfirmOpen(false))}
      >
        <DialogContent
          className="flex max-h-[calc(100vh-2rem)] flex-col overflow-hidden"
          hideClose={applying}
          onEscapeKeyDown={(event) => preventBusyConfirmationDismissal(applying, event)}
          onPointerDownOutside={(event) => preventBusyConfirmationDismissal(applying, event)}
          onInteractOutside={(event) => preventBusyConfirmationDismissal(applying, event)}
        >
          <DialogHeader className="shrink-0">
            <DialogTitle>{plan?.analysis.route === 'local_bridge' ? '启用本地桥接' : '应用适配配置'}</DialogTitle>
            <DialogDescription>
              {plan?.analysis.route === 'local_bridge'
                ? '桥接只监听本机，并会创建、切换 Codex Connection。AgentHub 需要保持在托盘运行；不会显示或复制来源凭据。'
                : `将为 ${source ? sourceLabel(source) : '所选连接'} 创建到 ${resolvedTargetAgentId ? targetAgentName(resolvedTargetAgentId) : '未选择目标'} 的适配。无需本地服务，也不会复制凭据。`}
            </DialogDescription>
          </DialogHeader>
          <div className="min-h-0 flex-1 space-y-4 overflow-y-auto pr-1">
            <AdapterPreviewList title="将写配置" values={plan?.changes ?? []} empty="没有需要写入的配置。" />
            {applyStageText ? <p className="text-xs text-secondary">当前阶段：{applyStageText}</p> : null}
            {applyError ? <AdapterErrorLines error={applyError} fallback="应用适配失败" /> : null}
          </div>
          <DialogFooter className="mt-4 shrink-0 border-t border-border pt-4">
            <Button variant="secondary" onClick={() => setApplyConfirmOpen(false)} disabled={applying}>取消</Button>
            <Button onClick={() => void confirmApply()} disabled={applying || !canApply}>
              {applying ? '应用中…' : '确认应用'}
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
            <DialogTitle>停止本地桥接？</DialogTitle>
            <DialogDescription>停止后，依赖此桥接的目标 Agent 将无法继续通过该本地端点访问来源连接。</DialogDescription>
          </DialogHeader>
          <div className="min-h-0 flex-1 space-y-3 overflow-y-auto pr-1">
            {stopConfirm && <p className="text-sm text-secondary">{adapterProfileRecordLabel(stopConfirm)}</p>}
            {stopError ? <AdapterErrorLines error={stopError} fallback="无法停止本地桥接" /> : null}
          </div>
          <DialogFooter className="mt-4 shrink-0 border-t border-border pt-4">
            <Button variant="secondary" onClick={() => setStopConfirm(null)} disabled={stopDialogBusy}>取消</Button>
            <Button variant="danger" onClick={() => void confirmStopBridge()} disabled={stopDialogBusy}>
              {stopDialogBusy ? '停止中…' : '确认停止'}
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
            <DialogTitle>删除此适配？</DialogTitle>
            <DialogDescription>
              会移除生成的适配配置与 Provider 投影。若它仍是当前 Connection，删除会被拒绝。
            </DialogDescription>
          </DialogHeader>
          <div className="min-h-0 flex-1 space-y-3 overflow-y-auto pr-1">
            {removeConfirm && <p className="text-sm text-secondary">{adapterProfileRecordLabel(removeConfirm)}</p>}
            {removeError ? <AdapterErrorLines error={removeError} fallback="无法删除此适配" /> : null}
          </div>
          <DialogFooter className="mt-4 shrink-0 border-t border-border pt-4">
            <Button variant="secondary" onClick={() => setRemoveConfirm(null)} disabled={removeDialogBusy}>取消</Button>
            <Button variant="danger" onClick={() => void confirmRemove()} disabled={removeDialogBusy}>
              {removeDialogBusy ? '删除中…' : removeError ? '重试删除' : '确认删除'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
