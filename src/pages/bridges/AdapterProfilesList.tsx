import { ArrowRight, Boxes, Copy } from 'lucide-react';
import { AgentDot } from '@/components/shared/AgentDot';
import { EmptyState } from '@/components/shared/EmptyState';
import { ErrorState } from '@/components/shared/ErrorState';
import { ListRow } from '@/components/shared/ListRow';
import { Button } from '@/components/ui/button';
import { Skeleton } from '@/components/ui/skeleton';
import { useToast } from '@/components/ui/toast';
import { agentDisplayName } from '@/config/agents';
import type {
  AdapterBridgeRuntimeStatus,
  AdapterProfile,
} from '@/lib/backend/contracts/adapter';
import type { ConnectionEntry } from '@/lib/connection-entry';
import { AdapterErrorLines } from './adapter-components';
import {
  adapterBridgeEndpointLabel,
  adapterFailurePresentation,
  BRIDGES_EMPTY_DESCRIPTION,
  BRIDGES_EMPTY_TITLE,
  BRIDGES_MUTATION_FAILURE,
} from './adapter-model';
import {
  adapterProfilePrimaryAction,
  adapterProfileRecoveryGuide,
  adapterStatusDotClass,
  adapterStatusTextClass,
  bridgeRuntimeStatusView,
  resolveAdapterProfileSource,
  type AdapterStatusView,
} from './adapter-view-model';

export type AdapterProfilesListProps = {
  profiles: AdapterProfile[];
  bridgeStatuses: Record<string, AdapterBridgeRuntimeStatus>;
  /** Per-profile bridge status *read* failures — shown as 状态不可用, not as a bridge fault. */
  statusErrors: Record<string, unknown>;
  /** Full (unfiltered) connection pool for source name resolution. */
  entries: ConnectionEntry[];
  loading: boolean;
  loadError: unknown;
  /** Per-profile mutation errors (start/stop/autostart/unbind). */
  errors: Record<string, unknown>;
  busyProfileIds: Record<string, boolean>;
  removingProfileId: string | null;
  onStartBridge: (profile: AdapterProfile) => void;
  onRequestStopBridge: (profile: AdapterProfile) => void;
  onShowDetail: (profile: AdapterProfile) => void;
  onRetry: () => void;
};

/**
 * Local-bridge runtimes as a compact service list. Row surface: single-layer
 * health, source → target, endpoint copy, and the state-matched primary action.
 */
export function AdapterProfilesList({
  profiles,
  bridgeStatuses,
  statusErrors,
  entries,
  loading,
  loadError,
  errors,
  busyProfileIds,
  removingProfileId,
  onStartBridge,
  onRequestStopBridge,
  onShowDetail,
  onRetry,
}: AdapterProfilesListProps) {
  if (loading) {
    return (
      <div className="space-y-2" aria-live="polite">
        <Skeleton className="h-16 w-full" />
        <Skeleton className="h-16 w-full" />
      </div>
    );
  }
  if (loadError) {
    return (
      <ErrorState
        error={loadError}
        title="无法读取本机桥"
        onRetry={onRetry}
      />
    );
  }
  if (profiles.length === 0) {
    return (
      <EmptyState
        icon={Boxes}
        title={BRIDGES_EMPTY_TITLE}
        description={BRIDGES_EMPTY_DESCRIPTION}
      />
    );
  }
  return (
    <div className="space-y-2">
      {profiles.map((profile) => (
        <AdapterProfileRow
          key={profile.id}
          profile={profile}
          bridgeStatus={profile.route === 'local_bridge' ? bridgeStatuses[profile.id] : undefined}
          statusUnavailable={Boolean(statusErrors[profile.id])}
          entries={entries}
          busy={busyProfileIds[profile.id] === true || removingProfileId === profile.id}
          error={errors[profile.id]}
          onStartBridge={onStartBridge}
          onRequestStopBridge={onRequestStopBridge}
          onShowDetail={onShowDetail}
        />
      ))}
    </div>
  );
}

function AdapterProfileRow({
  profile,
  bridgeStatus,
  statusUnavailable,
  entries,
  busy,
  error,
  onStartBridge,
  onRequestStopBridge,
  onShowDetail,
}: {
  profile: AdapterProfile;
  bridgeStatus?: AdapterBridgeRuntimeStatus;
  statusUnavailable: boolean;
  entries: ConnectionEntry[];
  busy: boolean;
  error: unknown;
  onStartBridge: (profile: AdapterProfile) => void;
  onRequestStopBridge: (profile: AdapterProfile) => void;
  onShowDetail: (profile: AdapterProfile) => void;
}) {
  const source = resolveAdapterProfileSource(profile, entries);
  const runtimeStatus = bridgeRuntimeStatusView({
    route: profile.route,
    bridgeState: bridgeStatus?.state,
    statusUnavailable,
  });
  const endpoint = profile.route === 'local_bridge'
    ? adapterBridgeEndpointLabel(profile, bridgeStatus)
    : null;
  const action = adapterProfilePrimaryAction({
    route: profile.route,
    bridgeState: bridgeStatus?.state,
    lastErrorCode: profile.lastErrorCode,
    statusUnavailable,
  });
  const transitioning = bridgeStatus?.state === 'starting' || bridgeStatus?.state === 'stopping';
  const recovery = adapterProfileRecoveryGuide(profile);
  const failure = error ? adapterFailurePresentation(error, BRIDGES_MUTATION_FAILURE) : null;

  return (
    <ListRow className="p-3">
      <div className="flex flex-col gap-2 lg:flex-row lg:items-center lg:gap-4">
        <div className="w-40 shrink-0">
          {runtimeStatus ? <StatusLine view={runtimeStatus} emphasis /> : null}
        </div>
        <div className="min-w-0 flex-1 space-y-1">
          <p className="flex min-w-0 flex-wrap items-center gap-1.5 text-sm font-medium">
            {source.agentId ? <AgentDot agentId={source.agentId} size="sm" title={null} /> : null}
            <span className="truncate">{source.title}</span>
            <ArrowRight className="h-3.5 w-3.5 shrink-0 text-muted" aria-hidden />
            <AgentDot agentId={profile.targetAgentId} size="sm" title={null} />
            <span className="truncate">{agentDisplayName(profile.targetAgentId)}</span>
          </p>
          <div className="flex min-w-0 flex-wrap items-center gap-1.5">
            {endpoint ? <EndpointCopy endpoint={endpoint} /> : null}
            {source.missing ? (
              <span className="text-xs text-warning">来源连接已删除</span>
            ) : null}
          </div>
        </div>
        <div className="flex shrink-0 items-center gap-2">
          {action ? (
            <Button
              variant="outline"
              size="sm"
              disabled={busy || transitioning}
              onClick={() => (action.kind === 'stop' ? onRequestStopBridge(profile) : onStartBridge(profile))}
            >
              {busy ? '处理中…' : action.label}
            </Button>
          ) : null}
          <Button variant="ghost" size="sm" onClick={() => onShowDetail(profile)}>
            详情
          </Button>
        </div>
      </div>
      {recovery ? (
        <p className="mt-2 text-xs text-warning" role="status">
          {recovery.summary} 打开「详情」查看步骤。
        </p>
      ) : null}
      {failure ? (
        <div className="mt-2 space-y-1" role="alert">
          <AdapterErrorLines error={error} fallback={BRIDGES_MUTATION_FAILURE} />
          <p className="text-xs text-secondary">{failure.hint}</p>
        </div>
      ) : null}
    </ListRow>
  );
}

function StatusLine({ view, emphasis = false }: { view: AdapterStatusView; emphasis?: boolean }) {
  return (
    <span className={emphasis ? 'flex items-center gap-1.5 text-sm' : 'flex items-center gap-1.5 text-xs'}>
      <span
        className={`inline-block h-2 w-2 shrink-0 rounded-full ${adapterStatusDotClass(view.tone)}${view.pulse ? ' animate-pulse' : ''}`}
        aria-hidden
      />
      <span className={adapterStatusTextClass(view.tone)}>{view.label}</span>
    </span>
  );
}

function EndpointCopy({ endpoint }: { endpoint: string }) {
  const { toast } = useToast();
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(`http://${endpoint}`);
      toast({ title: '端点已复制', description: `http://${endpoint}` });
    } catch {
      toast({ title: '复制失败', variant: 'danger' });
    }
  };
  return (
    <button
      type="button"
      className="inline-flex items-center gap-1 rounded-btn px-1 py-0.5 font-mono text-xs text-secondary hover:bg-hover hover:text-primary"
      onClick={() => { void copy(); }}
      aria-label={`复制本地端点 ${endpoint}`}
    >
      {endpoint}
      <Copy className="h-3 w-3" aria-hidden />
    </button>
  );
}
