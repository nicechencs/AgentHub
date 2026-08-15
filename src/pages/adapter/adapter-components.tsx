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
  errorMessage,
  futureAvailability,
  profileStatusBadge,
  routeLabel,
  supportBadge,
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
        <p className="text-sm text-secondary">正在分析并生成只读配置预览…</p>
        <Skeleton className="h-4 w-40" />
        <Skeleton className="h-4 w-full" />
      </div>
    );
  }
  if (error) {
    return (
      <ErrorState
        compact={compact}
        error={errorMessage(error, '无法分析此连接')}
        title="无法生成适配预览"
        onRetry={onRetry}
      />
    );
  }
  if (!analysis) return <p className="text-sm text-secondary">选择来源后自动生成预览。</p>;

  const support = supportBadge(analysis.support);
  if (analysis.route === 'unsupported') {
    return (
      <div className="space-y-4 text-sm">
        <div className="flex flex-wrap items-center gap-2">
          <h2 className="font-medium">暂未支持此组合</h2>
          <Badge variant={support.variant}>{support.label}</Badge>
          <ShieldCheck className="h-4 w-4 text-secondary" aria-label="不会执行更改" />
        </div>
        <p>{analysis.reason}</p>
        <section className="space-y-1 rounded-btn border border-border bg-subtle p-3 text-secondary">
          <h3 className="font-medium text-primary">暂未支持不等于连接失效</h3>
          <p>本次不会写入配置、启动服务或改变当前连接。</p>
          <p>下一步：继续使用原连接、改用目标 Agent 自身登录，或更换已支持的来源与目标组合。</p>
        </section>
        <EvidenceList evidence={analysis.evidence} />
      </div>
    );
  }

  const availability = onApply ? null : futureAvailability(analysis.route);
  return (
    <div className="space-y-4 text-sm">
      <div className="flex flex-wrap items-center gap-2">
        <h2 className="font-medium">{routeLabel(analysis.route)}</h2>
        <Badge variant={support.variant}>{support.label}</Badge>
        {availability && <Badge variant="warning">{availability}</Badge>}
        <ShieldCheck className="h-4 w-4 text-secondary" aria-label="只读预览" />
      </div>
      <p>{analysis.reason}</p>
      <AdapterPreviewList title="将写配置" values={plan?.changes ?? []} empty="此路径当前不会写入配置。" />
      <p className="text-xs text-secondary">
        服务影响：{plan?.serviceImpact === 'requires_local_bridge'
          ? '将启动仅本机可访问的协议桥接；请让 AgentHub 保持在托盘运行。'
          : '无需本地服务'}
      </p>
      {onApply && <Button onClick={onApply}>{analysis.route === 'local_bridge' ? '启用本地桥接' : '应用配置'}</Button>}
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
  hiddenTargetIds,
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
  hiddenTargetIds?: ReadonlySet<string>;
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
          const targetHidden = hiddenTargetIds?.has(profile.targetAgentId) === true;
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
                  <Button
                    variant="dangerOutline"
                    size="sm"
                    disabled={removing || busy || targetHidden}
                    title={targetHidden ? '目标 Agent 已隐藏，仅可停止运行中的桥接' : undefined}
                    onClick={() => onRemove(profile)}
                  >
                    {removing ? '删除中…' : '删除'}
                  </Button>
                </div>
              </div>
              {profile.route === 'local_bridge' && (
                <div className="mt-3 flex flex-wrap items-center gap-3 border-t border-border pt-3 text-sm">
                  <label className="flex items-center gap-2 text-secondary">
                    <Switch
                      checked={profile.autoStart}
                      disabled={busy || targetHidden}
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
                      disabled={busy || bridgeTransitioning || targetHidden}
                      onClick={() => onStartBridge(profile)}
                    >
                      {busy ? '处理中…' : '启动'}
                    </Button>
                  )}
                  <a className="text-info hover:underline" href="#/connections">在 Connections 查看</a>
                </div>
              )}
              {errors[profile.id] && <p className="mt-2 text-sm text-danger" role="alert">{errors[profile.id]}</p>}
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
