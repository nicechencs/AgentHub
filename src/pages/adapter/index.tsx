import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Boxes, ChevronRight } from 'lucide-react';
import { PageHeader } from '@/components/layout/PageHeader';
import { EmptyState } from '@/components/shared/EmptyState';
import { ErrorState } from '@/components/shared/ErrorState';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { AGENT_IDS } from '@/config/agents';
import {
  analyzeAdapter,
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
  AdapterProfile,
  AdapterRouteAnalysis,
} from '@/lib/backend/contracts/adapter';
import type { AgentId } from '@/lib/types';
import {
  AdapterPreviewList,
  AdapterPreviewResult,
  AdapterProfiles,
} from './adapter-components';
import {
  adapterApplyCommit,
  adapterPageViewState,
  adapterProfileRecordLabel,
  canApplyAdapterPlan,
  errorMessage,
  isCurrentAdapterPreviewRequest,
  resourceFailureMessage,
  sourceLabel,
  sourceStatusHint,
  targetAgentName,
} from './adapter-model';
import { useAdapterResources } from './use-adapter-resources';

export {
  adapterActionLabel,
  adapterApplyCommit,
  adapterBridgeEndpointLabel,
  adapterBridgeStateLabel,
  adapterBridgeUpstreamLabel,
  adapterPageViewState,
  adapterPlanChangeLabel,
  adapterProfileRecordLabel,
  adapterProfileStatusLabel,
  canApplyAdapterPlan,
  futureAvailability,
  isCurrentAdapterPreviewRequest,
  isSubscriptionGateUnsupported,
  maskedIdSuffix,
  routeLabel,
  sourceLabel,
  sourceStatusHint,
  unsupportedPresentation,
} from './adapter-model';

/** A controlled confirmation dialog must stay visible while its mutation is in flight. */
export function closeConfirmationOnOpenChange(
  open: boolean,
  busy: boolean,
  onClose: () => void,
): void {
  if (!open && !busy) onClose();
}

/** Radix dismissal events need an explicit preventDefault while a mutation is in flight. */
export function preventBusyConfirmationDismissal(
  busy: boolean,
  event: Pick<Event, 'preventDefault'>,
): void {
  if (busy) event.preventDefault();
}

/** Adapter compatibility preview and saved generated projections. */
export default function AdapterPage() {
  const {
    entries,
    profiles,
    bridgeStatuses,
    errors: resourceErrors,
    connectionState,
    profileState,
    loading,
    reload,
    updateBridgeStatus,
    updateProfile,
    removeProfile,
  } = useAdapterResources();
  const [sourceKey, setSourceKey] = useState('');
  const [targetAgentId, setTargetAgentId] = useState<AgentId>('claude');
  const [dialogOpen, setDialogOpen] = useState(false);
  const [analysis, setAnalysis] = useState<AdapterRouteAnalysis | null>(null);
  const [plan, setPlan] = useState<AdapterApplyPlan | null>(null);
  const [analyzing, setAnalyzing] = useState(false);
  const [analysisError, setAnalysisError] = useState<unknown>(null);
  const [applyConfirmOpen, setApplyConfirmOpen] = useState(false);
  const [applying, setApplying] = useState(false);
  const [applyError, setApplyError] = useState<unknown>(null);
  const [applySuccess, setApplySuccess] = useState<string | null>(null);
  const [removeConfirm, setRemoveConfirm] = useState<AdapterProfile | null>(null);
  const [stopConfirm, setStopConfirm] = useState<AdapterProfile | null>(null);
  const [removingProfileId, setRemovingProfileId] = useState<string | null>(null);
  const [profileErrors, setProfileErrors] = useState<Record<string, string>>({});
  const [busyProfileIds, setBusyProfileIds] = useState<Record<string, boolean>>({});
  const [retryToken, setRetryToken] = useState(0);
  const requestGeneration = useRef(0);

  const source = useMemo(
    () => entries.find((entry) => entry.key === sourceKey) ?? null,
    [entries, sourceKey],
  );
  const bridgeStatusErrors = useMemo(
    () => Object.fromEntries(
      Object.entries(resourceErrors.bridgeStatuses).map(([profileId, error]) => [
        profileId,
        errorMessage(error, '无法读取本地桥接运行状态'),
      ]),
    ),
    [resourceErrors.bridgeStatuses],
  );
  const visibleProfileErrors = { ...bridgeStatusErrors, ...profileErrors };

  // Every selection (and retry) starts both read-only operations. The generation
  // check prevents an old result from replacing the visible selection.
  useEffect(() => {
    const generation = ++requestGeneration.current;
    setAnalysis(null);
    setPlan(null);
    setAnalysisError(null);
    // A previous apply failure must not stick on a newly selected source/target.
    setApplyError(null);
    if (!source) {
      setAnalyzing(false);
      return;
    }
    const request = {
      sourceKind: source.source,
      sourceId: source.id,
      targetAgentId,
    } as const;
    setAnalyzing(true);
    void Promise.all([analyzeAdapter(request), planAdapter(request)])
      .then(([nextAnalysis, nextPlan]) => {
        if (!isCurrentAdapterPreviewRequest(generation, requestGeneration.current)) return;
        setAnalysis(nextAnalysis);
        setPlan(nextPlan);
      })
      .catch((error) => {
        if (isCurrentAdapterPreviewRequest(generation, requestGeneration.current)) setAnalysisError(error);
      })
      .finally(() => {
        if (isCurrentAdapterPreviewRequest(generation, requestGeneration.current)) setAnalyzing(false);
      });
  }, [source, targetAgentId, retryToken]);

  const preview = plan?.analysis ?? analysis;
  const retryPreview = () => setRetryToken((token) => token + 1);
  const canApply = canApplyAdapterPlan(plan);
  const applyRequest = source ? {
    sourceKind: source.source,
    sourceId: source.id,
    targetAgentId,
  } as const : null;

  const setProfileBusy = (profileId: string, busy: boolean) => {
    setBusyProfileIds((current) => ({ ...current, [profileId]: busy }));
  };

  const clearProfileError = (profileId: string) => {
    setProfileErrors((current) => {
      const { [profileId]: _ignored, ...remaining } = current;
      return remaining;
    });
  };

  const setBridgeStatusBestEffort = useCallback(async (profile: AdapterProfile) => {
    try {
      const status = await getAdapterBridgeStatus(profile.id);
      updateBridgeStatus(status);
    } catch (error) {
      setProfileErrors((current) => ({
        ...current,
        [profile.id]: errorMessage(error, '无法读取本地桥接运行状态'),
      }));
    }
  }, [updateBridgeStatus]);

  const confirmApply = async () => {
    if (!applyRequest || !canApply) return;
    setApplying(true);
    setApplyError(null);
    try {
      const result = await applyAdapter(applyRequest);
      const committed = adapterApplyCommit(result);
      // Applying is the committed operation. Close both dialogs and show success
      // before any optional runtime inspection can fail or block the refresh.
      setApplySuccess(committed.successMessage);
      setApplyConfirmOpen(false);
      setDialogOpen(false);
      if (committed.shouldProbeBridge) void setBridgeStatusBestEffort(result.profile);
      if (committed.shouldRefresh) void reload();
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
      void reload();
    } catch (error) {
      setProfileErrors((current) => ({ ...current, [profile.id]: errorMessage(error, '无法启动本地桥接') }));
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
      void reload();
    } catch (error) {
      setProfileErrors((current) => ({ ...current, [profile.id]: errorMessage(error, '无法停止本地桥接') }));
    } finally {
      setProfileBusy(profile.id, false);
    }
  };

  const handleSetBridgeAutoStart = async (profile: AdapterProfile, autoStart: boolean) => {
    setProfileBusy(profile.id, true);
    clearProfileError(profile.id);
    try {
      updateProfile(await setAdapterBridgeAutoStart(profile.id, autoStart));
      void reload();
    } catch (error) {
      setProfileErrors((current) => ({ ...current, [profile.id]: errorMessage(error, '无法更新自动启动') }));
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
      void reload();
    } catch (error) {
      setProfileErrors((errors) => ({ ...errors, [profileId]: errorMessage(error, '无法删除此适配') }));
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
    entriesCount: entries.length,
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
        description="复用已有连接；必要时启动本地协议转换。"
        descriptionTip="不会把一家 OAuth 凭据“转换”为另一家授权，也不会在日志记录请求正文。桥接仅监听本机 127.0.0.1。"
        actions={(
          <Button onClick={() => setDialogOpen(true)} disabled={loading || entries.length === 0}>
            新建适配 <ChevronRight className="h-4 w-4" />
          </Button>
        )}
      />

      <div className="space-y-4">
        {connectionWarning && <p className="text-sm text-warning" role="alert">{connectionWarning}</p>}
        {viewState === 'loading' ? (
          <Card>
            <CardContent className="space-y-3 pt-6">
              <div className="h-5 w-32 animate-pulse rounded bg-muted" />
              <div className="h-4 w-72 animate-pulse rounded bg-muted" />
            </CardContent>
          </Card>
        ) : viewState === 'error' ? (
          <ErrorState
            error={connectionLoadError}
            title="无法读取连接"
            onRetry={() => void reload()}
          />
        ) : viewState === 'empty' ? (
          <EmptyState
            icon={Boxes}
            title="把现有连接接入其他 Agent"
            description="先在 Connections 保存官方登录或 API Key，再创建适配预览。Adapter 只引用 connectionId，不复制凭据。"
            actionLabel="去 Connections"
            onAction={() => { window.location.hash = '#/connections'; }}
          />
        ) : viewState === 'choose' || !source ? (
          <EmptyState
            icon={Boxes}
            title="选择一个来源连接"
            description="选择目标 Agent 后，会自动分析路径并生成只读配置预览。不支持的组合会中性说明原因与替代路径。"
            actionLabel="新建适配"
            onAction={() => setDialogOpen(true)}
          />
        ) : (
          <Card>
            <CardHeader>
              <div className="min-w-0">
                <CardTitle>当前预览</CardTitle>
                <p className="mt-1 text-sm text-secondary">
                  {sourceLabel(source)} <ChevronRight className="inline h-3.5 w-3.5" /> {targetAgentName(targetAgentId)}
                </p>
                <p className="mt-1 text-xs text-muted">{sourceStatusHint(source)}</p>
              </div>
              <Button variant="outline" size="sm" onClick={() => setDialogOpen(true)}>
                更改选择
              </Button>
            </CardHeader>
            <CardContent>
              <AdapterPreviewResult
                analysis={preview}
                plan={plan}
                loading={analyzing}
                error={analysisError}
                onRetry={retryPreview}
                onApply={canApply ? () => setApplyConfirmOpen(true) : undefined}
                applyError={applyError}
              />
            </CardContent>
          </Card>
        )}

        {applySuccess && (
          <p className="text-sm text-success" role="status">
            {applySuccess} <a className="underline" href="#/connections">在 Connections 查看</a>
          </p>
        )}

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
      </div>

      <Dialog open={dialogOpen} onOpenChange={setDialogOpen}>
        <DialogContent className="flex max-h-[calc(100vh-2rem)] flex-col overflow-hidden">
          <DialogHeader className="shrink-0">
            <DialogTitle>新建适配</DialogTitle>
            <DialogDescription>
              选择来源和目标后立即生成只读分析与配置预览；不会显示或复制凭据。
            </DialogDescription>
          </DialogHeader>
          <div className="min-h-0 flex-1 space-y-4 overflow-y-auto pr-1">
            <label className="block text-sm font-medium">
              来源连接
              <select
                aria-label="来源连接"
                className="mt-1 w-full rounded-btn border border-border bg-panel px-3 py-2 text-sm"
                value={sourceKey}
                onChange={(event) => setSourceKey(event.target.value)}
              >
                <option value="">请选择连接</option>
                {entries.map((entry) => (
                  <option key={entry.key} value={entry.key}>
                    {sourceLabel(entry)} · {sourceStatusHint(entry)}
                  </option>
                ))}
              </select>
            </label>
            {source ? (
              <p className="text-xs text-secondary">
                来源状态：{sourceStatusHint(source)}。Adapter 只引用 connectionId / sourceId。
              </p>
            ) : (
              <p className="text-xs text-secondary">
                没有合适连接时，请先前往 Connections 添加官方登录或 API Key。
              </p>
            )}
            <label className="block text-sm font-medium">
              目标 Agent
              <select
                aria-label="目标 Agent"
                className="mt-1 w-full rounded-btn border border-border bg-panel px-3 py-2 text-sm"
                value={targetAgentId}
                onChange={(event) => setTargetAgentId(event.target.value as AgentId)}
              >
                {AGENT_IDS.map((agentId) => (
                  <option key={agentId} value={agentId}>{targetAgentName(agentId)}</option>
                ))}
              </select>
            </label>
            {source && (
              <AdapterPreviewResult
                analysis={preview}
                plan={plan}
                loading={analyzing}
                error={analysisError}
                onRetry={retryPreview}
                compact
                onApply={canApply ? () => setApplyConfirmOpen(true) : undefined}
                applyError={applyError}
              />
            )}
          </div>
          <DialogFooter className="mt-4 shrink-0 border-t border-border pt-4">
            <Button variant="secondary" onClick={() => setDialogOpen(false)}>完成</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

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
                : `将为 ${source ? sourceLabel(source) : '所选连接'} 创建到 ${targetAgentName(targetAgentId)} 的适配。无需本地服务，也不会复制凭据。`}
            </DialogDescription>
          </DialogHeader>
          <div className="min-h-0 flex-1 space-y-4 overflow-y-auto pr-1">
            <AdapterPreviewList title="将写配置" values={plan?.changes ?? []} empty="没有需要写入的配置。" />
            {applyError ? <p className="text-sm text-danger" role="alert">{errorMessage(applyError, '应用适配失败')}</p> : null}
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
            {stopError && <p className="text-sm text-danger" role="alert">{stopError}</p>}
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
            {removeError && <p className="text-sm text-danger" role="alert">{removeError}</p>}
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
