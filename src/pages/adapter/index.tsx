import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Link, useNavigate, useSearchParams } from 'react-router-dom';
import { ArrowRight, Boxes } from 'lucide-react';
import { AgentDot } from '@/components/shared/AgentDot';
import { PageHeader } from '@/components/layout/PageHeader';
import { PageSection } from '@/components/layout/PageSection';
import { CurrentBadge } from '@/components/shared/CurrentBadge';
import { EmptyState } from '@/components/shared/EmptyState';
import { Notice } from '@/components/shared/Notice';
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
import { AdapterProfileDetailDialog } from './AdapterProfileDetailDialog';
import { AdapterAgentBadge, AdapterSourceList } from './AdapterSourceList';
import { AdapterTargetGrid } from './AdapterTargetGrid';
import {
  adapterApplyCommit,
  adapterPageDescription,
  adapterPageViewState,
  adapterPlanRequestSignature,
  canApplyAdapterSelection,
  canConfirmAdapterApply,
  canRequestAdapterPlan,
  filterProfilesByCredential,
  isAdapterPlanMatchedToSelection,
  isCurrentAdapterPreviewRequest,
  parseAdapterCredentialFilter,
  resolveAdapterVisibleSourceKey,
  resourceFailureMessage,
  sourceLabel,
  sourceStatusHint,
  targetAgentName,
  type AdapterCredentialFilter,
  type AdapterPlanRequestSignature,
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
import {
  ADAPTER_SUPPORTED_PATH_EXAMPLES,
  adapterApplySummaryLine,
  adapterBridgeFleetSummary,
  adapterProfileFlowLabel,
  adapterRoutePipelineModel,
} from './adapter-view-model';
import { useAdapterResources } from './use-adapter-resources';
import { useAdapterTargetAnalyses } from './use-adapter-target-analyses';
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
  adapterAgentBadgeStyle,
  adapterApplyCommit,
  adapterBridgeEndpointLabel,
  adapterBridgeStateLabel,
  adapterBridgeUpstreamLabel,
  adapterCredentialFilterLabel,
  adapterCredentialKindLabel,
  adapterPageDescription,
  adapterPageViewState,
  adapterPlanChangeLabel,
  adapterPlanRequestSignature,
  adapterPreviewOutcome,
  adapterProfileRecordLabel,
  adapterProfileStatusLabel,
  adapterServiceImpactLabel,
  adapterTabDescription,
  adapterTabLabel,
  adapterTableRouteLabel,
  canApplyAdapterPlan,
  canApplyAdapterSelection,
  canConfirmAdapterApply,
  canRequestAdapterPlan,
  connectionKindForFilter,
  connectionKindForTab,
  filterProfilesByCredential,
  filterProfilesByMode,
  isAdapterPlanMatchedToSelection,
  isCurrentAdapterPreviewRequest,
  isSameAdapterPlanRequestSignature,
  isSubscriptionGateUnsupported,
  maskedIdSuffix,
  parseAdapterCredentialFilter,
  parseAdapterTab,
  resolveAdapterTargetAgentId,
  resolveAdapterVisibleSourceKey,
  routeLabel,
  sourceLabel,
  sourceStatusHint,
  unsupportedPresentation,
} from './adapter-model';

export {
  adapterApplySummaryLine,
  adapterBridgeFleetSummary,
  adapterConfigStatusView,
  adapterProfileFlowLabel,
  adapterProfilePrimaryAction,
  adapterProfileRecoveryGuide,
  adapterRoutePipelineModel,
  adapterServiceStatusView,
  adapterTargetBadge,
  adapterTargetCacheKey,
  resolveAdapterProfileSource,
} from './adapter-view-model';

export {
  closeConfirmationOnOpenChange,
  preventBusyConfirmationDismissal,
} from '@/components/shared/busy-confirmation';

function connectionsHref(filter: AdapterCredentialFilter): string {
  if (filter === 'oauth') return '/connections?mode=oauth';
  if (filter === 'apikey') return '/connections?mode=apikey';
  return '/connections';
}

/**
 * Adapter page: pick a saved connection, fan out a read-only route analysis
 * over every configurable target Agent, preview the selected path as a
 * pipeline, and manage the generated projections as a compact service list.
 */
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
  /** Signature the currently stored plan was requested for; null means no usable plan. */
  const [planSignature, setPlanSignature] = useState<AdapterPlanRequestSignature | null>(null);
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
  const [detailProfileId, setDetailProfileId] = useState<string | null>(null);
  const [removingProfileId, setRemovingProfileId] = useState<string | null>(null);
  const [profileErrors, setProfileErrors] = useState<Record<string, unknown>>({});
  const [busyProfileIds, setBusyProfileIds] = useState<Record<string, boolean>>({});
  const [retryToken, setRetryToken] = useState(0);
  const requestGeneration = useRef(0);
  const sourcePanelRef = useRef<HTMLDivElement | null>(null);

  const configurableTargetIds = useMemo(
    () => selectableTargetAgentIds({
      state: agentStatusSnapshot.state,
      statuses: agentStatusSnapshot.statuses,
      fallbackIds: AGENT_IDS,
    }),
    [agentStatusSnapshot.state, agentStatusSnapshot.statuses],
  );
  const configurableTargetIdSet = useMemo(
    () => new Set<AgentId>(configurableTargetIds),
    [configurableTargetIds],
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

  // Selection is only valid when still visible under the active filter/search.
  // An invisible source must not keep driving the target pane or apply path.
  const visibleSourceKey = resolveAdapterVisibleSourceKey(sourceKey, filteredEntries);
  const source = useMemo(
    () => filteredEntries.find((entry) => entry.key === visibleSourceKey) ?? null,
    [filteredEntries, visibleSourceKey],
  );
  const sourceAuthIncomplete = isOAuthAuthIncomplete(source);
  const sourceBadge = source ? kindBadge(source.kind) : null;

  // Target selection is explicit (panorama card click); it only stays
  // effective while the target is still configurable on this machine.
  const resolvedTargetAgentId: AgentId | '' = targetAgentId && configurableTargetIdSet.has(targetAgentId)
    ? targetAgentId
    : '';

  useEffect(() => {
    if (sourceKey && sourceKey !== visibleSourceKey) {
      setSourceKey('');
    }
  }, [sourceKey, visibleSourceKey]);

  // Incomplete OAuth blocks every analysis / plan / apply, so the fan-out
  // must not run either — the only CTA is finishing login in Connections.
  const fanOutEnabled = Boolean(source) && !sourceAuthIncomplete;
  const { analyses: targetAnalyses, retry: retryTargetAnalysis } = useAdapterTargetAnalyses({
    sourceKind: source?.source ?? null,
    sourceId: source?.id ?? null,
    targetAgentIds: configurableTargetIds,
    enabled: fanOutEnabled,
  });

  // Every selection (and retry) starts a plan request. Generation drops late
  // responses; planSignature additionally binds a stored plan to its request.
  useEffect(() => {
    const generation = ++requestGeneration.current;
    setPlan(null);
    setPlanSignature(null);
    setAnalysisError(null);
    // A previous apply failure or success must not stick on a newly selected source/target.
    setApplyError(null);
    setApplySuccess(null);
    setApplyResultProfile(null);
    setApplyProbeStatus(null);
    // Stale confirm dialog must not stay open for a different selection.
    setApplyConfirmOpen(false);
    if (!source || !resolvedTargetAgentId || sourceAuthIncomplete || !canRequestAdapterPlan({
      sourceId: source.id,
      targetAgentId: resolvedTargetAgentId,
    })) {
      setAnalyzing(false);
      return;
    }
    const request = adapterPlanRequestSignature({
      sourceKind: source.source,
      sourceId: source.id,
      targetAgentId: resolvedTargetAgentId,
    });
    setAnalyzing(true);
    void planAdapter(request)
      .then((nextPlan) => {
        if (!isCurrentAdapterPreviewRequest(generation, requestGeneration.current)) return;
        setPlan(nextPlan);
        setPlanSignature(request);
      })
      .catch((error) => {
        if (isCurrentAdapterPreviewRequest(generation, requestGeneration.current)) setAnalysisError(error);
      })
      .finally(() => {
        if (isCurrentAdapterPreviewRequest(generation, requestGeneration.current)) setAnalyzing(false);
      });
  }, [resolvedTargetAgentId, retryToken, source, sourceAuthIncomplete]);

  const currentRequestSignature = source && resolvedTargetAgentId
    ? adapterPlanRequestSignature({
      sourceKind: source.source,
      sourceId: source.id,
      targetAgentId: resolvedTargetAgentId,
    })
    : null;
  const matchedPlan = isAdapterPlanMatchedToSelection(plan, planSignature, currentRequestSignature)
    ? plan
    : null;
  const preview = matchedPlan?.analysis ?? null;
  const retryPreview = () => setRetryToken((token) => token + 1);
  const canApply = canApplyAdapterSelection({
    plan: matchedPlan,
    planSignature,
    currentSignature: currentRequestSignature,
    authIncomplete: sourceAuthIncomplete,
  });
  const applyRequest = currentRequestSignature;
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

  const previewPipeline = source && resolvedTargetAgentId && preview
    ? adapterRoutePipelineModel({
      sourceTitle: source.title,
      sourceAgentId: source.agentId,
      credentialLabel: sourceBadge?.label ?? '',
      targetAgentId: resolvedTargetAgentId,
      route: preview.route,
    })
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
    // Re-validate signature at submit time so a dialog opened for A cannot apply B.
    if (!canConfirmAdapterApply({
      applyRequest,
      plan: matchedPlan,
      planSignature,
      authIncomplete: sourceAuthIncomplete,
    })) {
      setApplyConfirmOpen(false);
      return;
    }
    if (!applyRequest) return;
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

  // `?tab=` drives both the source picker and the managed-profile list.
  const visibleProfiles = useMemo(
    () => filterProfilesByCredential(profiles, filter),
    [profiles, filter],
  );
  const fleetSummary = adapterBridgeFleetSummary(visibleProfiles, bridgeStatuses);
  const profilesFilteredOut = !loading
    && profileState !== 'error'
    && visibleProfiles.length === 0
    && profiles.length > 0;

  const detailProfile = detailProfileId
    ? profiles.find((profile) => profile.id === detailProfileId) ?? null
    : null;

  const scrollToSources = () => {
    sourcePanelRef.current?.scrollIntoView({ behavior: 'smooth', block: 'start' });
  };

  return (
    <div>
      <PageHeader
        title="Adapter"
        description={adapterPageDescription()}
        descriptionTip="凭据保存在 Connections，不会展示或复制。本地桥接只监听 127.0.0.1，日志不记请求正文。"
        actions={(
          <Button variant="outline" onClick={() => navigate(connectionsHref(filter))}>
            去 Connections
          </Button>
        )}
      />

      {connectionWarning && <p className="mb-3 text-sm text-warning" role="alert">{connectionWarning}</p>}

      <div className="grid grid-cols-1 gap-4 lg:grid-cols-[minmax(16rem,20rem)_minmax(0,1fr)] lg:items-start" ref={sourcePanelRef}>
        {/*
          Wide: sticky master column with viewport-bounded height.
          Narrow: stacked layout with a capped list height so the target pane
          is reachable without scrolling past the full connection list.
        */}
        <Card className="flex max-h-[min(24rem,55vh)] min-h-0 flex-col overflow-hidden lg:sticky lg:top-6 lg:max-h-[calc(100vh-8rem)] lg:min-h-[24rem]">
          <CardContent className="flex min-h-0 flex-1 flex-col p-4">
            <AdapterSourceList
              groups={sourceGroups}
              selectedKey={visibleSourceKey}
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
              <CardTitle>接入目标</CardTitle>
              <p className="mt-1 text-sm text-secondary">
                {source
                  ? '点选目标，查看接入路径。'
                  : '选左侧连接，这里会分析可接入的目标。'}
              </p>
            </div>
          </CardHeader>
          <CardContent className="space-y-4">
            {!source ? (
              <div className="space-y-4">
                <EmptyState
                  icon={Boxes}
                  title="选择一个来源连接"
                  description="选中后自动分析可接入的目标。"
                />
                <section className="space-y-1.5">
                  <h3 className="text-xs font-medium text-secondary">当前支持一键接入</h3>
                  <div className="space-y-1.5">
                    {ADAPTER_SUPPORTED_PATH_EXAMPLES.map((example) => (
                      <div
                        key={`${example.source}-${example.targetAgentId}`}
                        className="flex flex-wrap items-center gap-1.5 rounded-btn border border-border bg-subtle/60 px-3 py-2 text-sm"
                      >
                        <span>{example.source}</span>
                        <ArrowRight className="h-3.5 w-3.5 text-muted" aria-hidden />
                        <AgentDot agentId={example.targetAgentId} size="sm" title={null} />
                        <span>{targetAgentName(example.targetAgentId)}</span>
                        <Badge variant={example.badge.variant}>{example.badge.label}</Badge>
                      </div>
                    ))}
                  </div>
                  <p className="text-xs text-muted">其余组合会说明原因。</p>
                </section>
              </div>
            ) : (
              <>
                <section className="space-y-2 rounded-btn border border-border bg-subtle p-3">
                  <div className="flex flex-wrap items-center gap-1.5">
                    <p className="truncate text-sm font-medium">{source.title}</p>
                    <AdapterAgentBadge agentId={source.agentId} />
                    {sourceBadge ? <Badge variant={sourceBadge.variant}>{sourceBadge.label}</Badge> : null}
                    {source.isCurrent ? <CurrentBadge /> : null}
                  </div>
                  <p className="text-xs text-muted">{sourceStatusHint(source)}</p>
                </section>

                {sourceAuthIncomplete ? (
                  <Notice
                    tone="warning"
                    actionLabel="去 Connections"
                    onAction={() => navigate('/connections')}
                  >
                    {oauthIncompleteAuthHint()} 完成授权后再分析可接入的目标。
                  </Notice>
                ) : (
                  <>
                    <section className="space-y-1.5">
                      <h3 className="text-sm font-medium">目标 Agent</h3>
                      {configurableTargetIds.length === 0 ? (
                        <p className="text-sm text-secondary">
                          没有可配置的目标。先到 <Link className="text-info underline" to="/agents">Agents</Link> 安装或修复。
                        </p>
                      ) : (
                        <AdapterTargetGrid
                          agentIds={AGENT_IDS}
                          configurableIds={configurableTargetIdSet}
                          analyses={targetAnalyses}
                          selectedAgentId={resolvedTargetAgentId}
                          onSelect={setTargetAgentId}
                          onRetry={retryTargetAnalysis}
                        />
                      )}
                    </section>

                    {resolvedTargetAgentId ? (
                      <>
                        <AdapterPreviewResult
                          analysis={preview}
                          plan={matchedPlan}
                          loading={analyzing}
                          error={analysisError}
                          onRetry={retryPreview}
                          onApply={canApply ? () => setApplyConfirmOpen(true) : undefined}
                          applyError={applyError}
                          pipeline={previewPipeline}
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
                    ) : configurableTargetIds.length > 0 ? (
                      <p className="text-sm text-secondary">点选一个目标，查看接入路径。</p>
                    ) : null}
                  </>
                )}
              </>
            )}
          </CardContent>
        </Card>
      </div>

      <PageSection
        title="已创建的适配"
        description="管理已生效的接入与本地桥接。"
        ruled
      >
        {fleetSummary ? (
          <p className="mb-3 text-xs text-secondary">
            {fleetSummary.label} · 需保持托盘运行。
          </p>
        ) : null}
        {profilesFilteredOut ? (
          <p className="text-sm text-secondary">
            当前筛选下没有适配。可切换筛选查看全部 {profiles.length} 条。
          </p>
        ) : (
          <AdapterProfiles
            profiles={visibleProfiles}
            bridgeStatuses={bridgeStatuses}
            statusErrors={resourceErrors.bridgeStatuses}
            entries={entries}
            loading={loading && profiles.length === 0}
            loadError={profileState === 'error' ? resourceErrors.profiles : null}
            errors={profileErrors}
            busyProfileIds={busyProfileIds}
            removingProfileId={removingProfileId}
            onStartBridge={handleStartBridge}
            onRequestStopBridge={setStopConfirm}
            onShowDetail={(profile) => setDetailProfileId(profile.id)}
            onRetry={() => void reload()}
            onStartCreate={scrollToSources}
          />
        )}
      </PageSection>

      <AdapterProfileDetailDialog
        profile={detailProfile}
        bridgeStatus={detailProfile ? bridgeStatuses[detailProfile.id] : undefined}
        statusUnavailable={detailProfile ? Boolean(resourceErrors.bridgeStatuses[detailProfile.id]) : false}
        entries={entries}
        busy={detailProfile
          ? busyProfileIds[detailProfile.id] === true || removingProfileId === detailProfile.id
          : false}
        error={detailProfile ? profileErrors[detailProfile.id] : null}
        onClose={() => setDetailProfileId(null)}
        onSetAutoStart={handleSetBridgeAutoStart}
        onRequestRemove={(profile) => {
          setDetailProfileId(null);
          setRemoveConfirm(profile);
        }}
      />

      <Dialog
        open={applyConfirmOpen && canApply}
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
            <DialogTitle>{matchedPlan?.analysis.route === 'local_bridge' ? '启用本地桥接' : '应用适配配置'}</DialogTitle>
            <DialogDescription>
              {matchedPlan?.analysis.route === 'local_bridge'
                ? '会创建本机桥接并切换目标连接。需保持托盘运行。'
                : `把 ${source ? sourceLabel(source) : '所选连接'} 接入 ${resolvedTargetAgentId ? targetAgentName(resolvedTargetAgentId) : '未选择目标'}。`}
            </DialogDescription>
          </DialogHeader>
          <div className="min-h-0 flex-1 space-y-4 overflow-y-auto pr-1">
            {matchedPlan && source && resolvedTargetAgentId ? (
              <p className="rounded-btn border border-border bg-subtle px-3 py-2 text-sm">
                {adapterApplySummaryLine({
                  sourceTitle: source.title,
                  targetAgentId: resolvedTargetAgentId,
                  route: matchedPlan.analysis.route,
                })}
              </p>
            ) : null}
            <AdapterPreviewList title="预计改动" values={matchedPlan?.changes ?? []} empty="无需写入配置。" />
            <p className="text-xs text-muted">失败会回滚到应用前，原有连接不受影响。</p>
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
            <DialogDescription>停止后，目标 Agent 将无法访问此桥接。</DialogDescription>
          </DialogHeader>
          <div className="min-h-0 flex-1 space-y-3 overflow-y-auto pr-1">
            {stopConfirm && <p className="text-sm text-secondary">{adapterProfileFlowLabel(stopConfirm, entries)}</p>}
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
              会移除适配配置。若仍是当前 Connection，删除会被拒绝。
            </DialogDescription>
          </DialogHeader>
          <div className="min-h-0 flex-1 space-y-3 overflow-y-auto pr-1">
            {removeConfirm && <p className="text-sm text-secondary">{adapterProfileFlowLabel(removeConfirm, entries)}</p>}
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
