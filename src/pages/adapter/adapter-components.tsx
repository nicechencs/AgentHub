import { useState } from 'react';
import { ExternalLink, ShieldCheck } from 'lucide-react';
import { ErrorState } from '@/components/shared/ErrorState';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Skeleton } from '@/components/ui/skeleton';
import { Switch } from '@/components/ui/switch';
import { openExternalLink } from '@/lib/open-external';
import type {
  AdapterApplyPlan,
  AdapterBridgeRuntimeStatus,
  AdapterProfile,
  AdapterRouteAnalysis,
} from '@/lib/backend/contracts/adapter';
import {
  adapterActionLabel,
  adapterBridgeEndpointLabel,
  adapterPlanChangeLabel,
  adapterProfileRecordLabel,
  bridgeStatusBadge,
  canApplyAdapterPlan,
  errorMessage,
  futureAvailability,
  profileStatusBadge,
  routeLabel,
  supportBadge,
  unsupportedPresentation,
} from './adapter-model';

/** A degraded bridge still owns its local listener and must be stopped, not started again. */
export function isBridgeStopCapable(
  state: AdapterBridgeRuntimeStatus['state'] | undefined,
): boolean {
  return state === 'running' || state === 'degraded';
}

/** Small injectable seam that keeps the Adapter evidence path on the Tauri-safe opener. */
export async function openAdapterEvidence(
  url: string,
  opener: (target: string) => Promise<void> = openExternalLink,
): Promise<void> {
  await opener(url);
}

export function AdapterPreviewResult({
  analysis,
  plan,
  loading,
  error,
  onRetry,
  compact = false,
  onApply,
  applyError,
}: {
  analysis: AdapterRouteAnalysis | null;
  plan: AdapterApplyPlan | null;
  loading: boolean;
  error: unknown;
  onRetry: () => void;
  compact?: boolean;
  onApply?: () => void;
  applyError?: unknown;
}) {
  if (loading) {
    return (
      <div className="space-y-2" aria-live="polite">
        <p className="text-sm text-secondary">正在分析路径并生成只读配置预览…</p>
        <p className="text-xs text-muted">仅使用 connectionId / sourceId；不会读取、展示或记录原始凭据。</p>
        <Skeleton className="h-4 w-40" />
        <Skeleton className="h-4 w-full" />
      </div>
    );
  }
  if (error) {
    return (
      <div className="space-y-2">
        <ErrorState
          compact={compact}
          error={errorMessage(error, '无法分析此连接')}
          title="无法生成适配预览"
          onRetry={onRetry}
        />
        <p className="text-xs text-secondary">
          这是分析失败，不是连接失效。可重试；若持续失败，请回到 Connections 确认来源状态后再试。
        </p>
      </div>
    );
  }
  if (!analysis) return <p className="text-sm text-secondary">选择来源后自动生成预览。</p>;

  const support = supportBadge(analysis.support);
  if (analysis.route === 'unsupported') {
    // Unsupported is a neutral gate conclusion — never a red fault, never Apply/Bridge.
    const presentation = unsupportedPresentation(analysis, plan);
    return (
      <div className="space-y-4 text-sm">
        <div className="flex flex-wrap items-center gap-2">
          <h2 className="font-medium">{presentation.headline}</h2>
          <Badge variant="default">{presentation.badgeLabel}</Badge>
          <Badge variant="default">plan.canApply=false</Badge>
          <ShieldCheck className="h-4 w-4 text-secondary" aria-label="不会执行更改" />
        </div>
        <p className="text-primary">{presentation.reason}</p>
        <section className="space-y-2 rounded-btn border border-border bg-subtle p-3 text-secondary">
          <h3 className="font-medium text-primary">门禁说明</h3>
          <ul className="list-disc space-y-1 pl-5">
            {presentation.gateLines.map((line) => (
              <li key={line}>{line}</li>
            ))}
          </ul>
          <p className="text-xs">{presentation.safetyNote}</p>
        </section>
        <section className="space-y-1">
          <h3 className="font-medium">可用替代路径</h3>
          <ul className="list-disc space-y-1 pl-5 text-secondary">
            {presentation.alternatives.map((line) => (
              <li key={line}>{line}</li>
            ))}
          </ul>
        </section>
        {analysis.limitations.length > 0 ? (
          <StringList title="限制" values={analysis.limitations} empty="无额外限制。" />
        ) : null}
        <EvidenceList evidence={analysis.evidence} />
      </div>
    );
  }

  const canApply = canApplyAdapterPlan(plan);
  const availability = canApply ? null : futureAvailability(analysis.route);
  // Only the backend canApply gate may surface mutation controls.
  const showApply = Boolean(onApply) && canApply;
  return (
    <div className="space-y-4 text-sm">
      <div className="flex flex-wrap items-center gap-2">
        <h2 className="font-medium">{routeLabel(analysis.route)}</h2>
        <Badge variant={support.variant}>{support.label}</Badge>
        {availability && <Badge variant="warning">{availability}</Badge>}
        {!canApply && plan ? <Badge variant="default">plan.canApply=false</Badge> : null}
        <ShieldCheck className="h-4 w-4 text-secondary" aria-label="只读预览" />
      </div>
      <p>{analysis.reason}</p>
      <AdapterPreviewList title="将写配置" values={plan?.changes ?? []} empty="此路径当前不会写入配置。" />
      <p className="text-xs text-secondary">
        服务影响：{plan?.serviceImpact === 'requires_local_bridge'
          ? '将启动仅本机可访问的协议桥接；请让 AgentHub 保持在托盘运行。'
          : '无需本地服务'}
      </p>
      {showApply && (
        <Button onClick={onApply}>{analysis.route === 'local_bridge' ? '启用本地桥接' : '应用配置'}</Button>
      )}
      {!canApply && !availability ? (
        <p className="text-xs text-secondary">当前路径不可应用（plan.canApply=false），仅展示只读预览。</p>
      ) : null}
      {applyError ? <p className="text-sm text-danger" role="alert">{errorMessage(applyError, '应用适配失败')}</p> : null}
      <AdapterActionList actions={analysis.actions} />
      <StringList title="限制" values={analysis.limitations} empty="无额外限制。" />
      <EvidenceList evidence={analysis.evidence} />
    </div>
  );
}

export function AdapterProfiles({
  profiles,
  bridgeStatuses,
  loading,
  loadError,
  errors,
  removingProfileId,
  busyProfileIds,
  onRemove,
  onStartBridge,
  onRequestStopBridge,
  onSetBridgeAutoStart,
  onRetry,
}: {
  profiles: AdapterProfile[];
  bridgeStatuses: Record<string, AdapterBridgeRuntimeStatus>;
  loading: boolean;
  loadError: unknown;
  errors: Record<string, string>;
  removingProfileId: string | null;
  busyProfileIds: Record<string, boolean>;
  onRemove: (profile: AdapterProfile) => void;
  onStartBridge: (profile: AdapterProfile) => void;
  onRequestStopBridge: (profile: AdapterProfile) => void;
  onSetBridgeAutoStart: (profile: AdapterProfile, autoStart: boolean) => void;
  onRetry: () => void;
}) {
  return (
    <Card>
      <CardHeader>
        <div>
          <CardTitle>已创建的适配</CardTitle>
          <p className="mt-1 text-sm text-secondary">生成的 Provider 仅引用原 Connection，不含凭据。</p>
        </div>
      </CardHeader>
      <CardContent className="space-y-3">
        {loading ? <Skeleton className="h-12 w-full" /> : loadError ? (
          <div className="space-y-2" role="alert">
            <p className="text-sm text-danger">{errorMessage(loadError, '无法读取已创建的适配。')}</p>
            <Button variant="outline" size="sm" onClick={onRetry}>重试</Button>
          </div>
        ) : profiles.length === 0 ? (
          <p className="text-sm text-secondary">尚未创建适配。</p>
        ) : profiles.map((profile) => {
          const status = profileStatusBadge(profile.status);
          const removing = removingProfileId === profile.id;
          const bridgeStatus = profile.route === 'local_bridge' ? bridgeStatuses[profile.id] : undefined;
          const bridgeBadge = bridgeStatusBadge(bridgeStatus?.state);
          const bridgeEndpoint = adapterBridgeEndpointLabel(profile, bridgeStatus);
          const busy = busyProfileIds[profile.id] === true;
          const bridgeTransitioning = bridgeStatus?.state === 'starting' || bridgeStatus?.state === 'stopping';
          return (
            <div key={profile.id} className="rounded-btn border border-border px-3 py-2">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <div className="min-w-0">
                  <p className="font-medium">{adapterProfileRecordLabel(profile)}</p>
                  <p className="mt-0.5 text-xs text-secondary">
                    {routeLabel(profile.route)} · 生成 Provider：{profile.generatedProviderId ?? '无'}
                  </p>
                  {profile.route === 'local_bridge' && (
                    <p className="mt-0.5 text-xs text-secondary">
                      本机桥接{bridgeEndpoint ? ` · ${bridgeEndpoint}` : ' · 等待分配端口'}
                      {bridgeStatus?.upstreamStatus ? ` · 上游：${bridgeStatus.upstreamStatus}` : ''}
                    </p>
                  )}
                </div>
                <div className="flex items-center gap-2">
                  <Badge variant={status.variant}>{status.label}</Badge>
                  {profile.route === 'local_bridge' && <Badge variant={bridgeBadge.variant}>{bridgeBadge.label}</Badge>}
                  <Button variant="dangerOutline" size="sm" disabled={removing || busy} onClick={() => onRemove(profile)}>
                    {removing ? '删除中…' : '删除'}
                  </Button>
                </div>
              </div>
              {profile.route === 'local_bridge' && (
                <div className="mt-3 flex flex-wrap items-center gap-3 border-t border-border pt-3 text-sm">
                  <label className="flex items-center gap-2 text-secondary">
                    <Switch
                      checked={profile.autoStart}
                      disabled={busy}
                      aria-label={`${adapterProfileRecordLabel(profile)} 自动启动`}
                      onCheckedChange={(autoStart) => onSetBridgeAutoStart(profile, autoStart)}
                    />
                    自动启动
                  </label>
                  {isBridgeStopCapable(bridgeStatus?.state) ? (
                    <Button
                      variant="outline"
                      size="sm"
                      disabled={busy || bridgeTransitioning}
                      onClick={() => onRequestStopBridge(profile)}
                    >
                      {busy ? '处理中…' : '停止'}
                    </Button>
                  ) : (
                    <Button
                      variant="outline"
                      size="sm"
                      disabled={busy || bridgeTransitioning}
                      onClick={() => onStartBridge(profile)}
                    >
                      {busy ? '处理中…' : bridgeStatus?.state === 'error' ? '重试启动' : '启动'}
                    </Button>
                  )}
                  <a className="text-info hover:underline" href="#/connections">在 Connections 查看</a>
                </div>
              )}
              {bridgeStatus?.state === 'degraded' && (
                <p className="mt-2 text-xs text-warning" role="status">
                  服务降级：本地监听可能仍在，但上游健康检查未通过。可先停止再启动；不会自动反复重试写配置。
                </p>
              )}
              {profile.status === 'needs_attention' && (
                <p className="mt-2 text-xs text-warning" role="status">
                  需要处理：上次操作可能部分完成。请按错误提示恢复，或删除后重新创建；不会自动反复重试。
                </p>
              )}
              {errors[profile.id] && (
                <div className="mt-2 space-y-1" role="alert">
                  <p className="text-sm text-danger">{errors[profile.id]}</p>
                  <p className="text-xs text-secondary">行内错误不会展示凭据；可重试当前操作或查看 Connections 中的来源连接状态。</p>
                </div>
              )}
            </div>
          );
        })}
      </CardContent>
    </Card>
  );
}

export function AdapterPreviewList({
  title,
  values,
  empty,
}: {
  title: string;
  values: AdapterApplyPlan['changes'];
  empty: string;
}) {
  return (
    <section>
      <h3 className="font-medium">{title}</h3>
      {values.length ? (
        <ul className="mt-1 list-disc space-y-1 pl-5 text-secondary">
          {values.map((change) => (
            <li key={`${change.target}-${change.field}`}>
              {adapterPlanChangeLabel(change)}
            </li>
          ))}
        </ul>
      ) : <p className="mt-1 text-secondary">{empty}</p>}
    </section>
  );
}

function AdapterActionList({ actions }: Pick<AdapterRouteAnalysis, 'actions'>) {
  return (
    <section>
      <h3 className="font-medium">预览动作</h3>
      {actions.length ? (
        <ul className="mt-1 list-disc space-y-1 pl-5 text-secondary">
          {actions.map((item) => (
            <li key={`${item.kind}-${item.target}-${item.description}`}>
              {adapterActionLabel(item)}
            </li>
          ))}
        </ul>
      ) : <p className="mt-1 text-secondary">没有可执行动作。</p>}
    </section>
  );
}

function StringList({ title, values, empty }: { title: string; values: string[]; empty: string }) {
  return (
    <section>
      <h3 className="font-medium">{title}</h3>
      {values.length ? (
        <ul className="mt-1 list-disc space-y-1 pl-5 text-secondary">
          {values.map((value) => <li key={value}>{value}</li>)}
        </ul>
      ) : <p className="mt-1 text-secondary">{empty}</p>}
    </section>
  );
}

function EvidenceList({ evidence }: Pick<AdapterRouteAnalysis, 'evidence'>) {
  const [openError, setOpenError] = useState<unknown>(null);

  const openEvidence = async (url: string) => {
    setOpenError(null);
    try {
      await openAdapterEvidence(url);
    } catch (error) {
      setOpenError(error);
    }
  };

  return (
    <section>
      <h3 className="font-medium">兼容性说明</h3>
      {evidence.length ? (
        <ul className="mt-1 space-y-1 text-secondary">
          {evidence.map((item) => (
            <li key={item.url}>
              <button
                type="button"
                className="inline-flex items-center gap-1 text-info hover:underline"
                onClick={() => { void openEvidence(item.url); }}
              >
                {item.label} <ExternalLink className="h-3 w-3" />
              </button>
              <span className="ml-1 text-xs">验证于 {item.verifiedAt}</span>
            </li>
          ))}
        </ul>
      ) : <p className="mt-1 text-secondary">无可展示依据。</p>}
      {openError ? (
        <p className="mt-2 text-sm text-danger" role="alert">
          {errorMessage(openError, '无法打开外部链接')}
        </p>
      ) : null}
    </section>
  );
}
