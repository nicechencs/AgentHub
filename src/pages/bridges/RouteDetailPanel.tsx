import { ArrowRight, ChevronDown, Copy } from 'lucide-react';
import { AgentDot } from '@/components/shared/AgentDot';
import { DetailRow } from '@/components/shared/DetailRow';
import { useI18n } from '@/components/shared/LanguageProvider';
import { StatusPin } from '@/components/shared/StatusPin';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Switch } from '@/components/ui/switch';
import { Hint } from '@/components/ui/tooltip';
import { useToast } from '@/components/ui/toast';
import { openLogsDir } from '@/lib/api/settings';
import type {
  AdapterBridgeRuntimeStatus,
  AdapterProfile,
} from '@/lib/backend/contracts/adapter';
import type { ConnectionEntry } from '@/lib/connection-entry';
import { cn } from '@/lib/utils';
import { AdapterErrorLines } from './adapter-components';
import { readCreateRouteCapabilities } from './create-route-flow';
import {
  adapterBridgeHostPort,
  adapterBridgeUpstreamLabel,
  adapterCredentialKindLabel,
} from './adapter-model';
import {
  bridgeHostPortLabel,
  bridgeNodeStatusLine,
  routeCopyPortPendingLabel,
  routeDetailTargetLabel,
  routeModelsSummary,
  routeSourceDeletedHint,
  upstreamChannelLabel,
} from './adapter-route-detail-model';
import {
  buildRouteGraph,
  routeGraphLinkLabel,
  type RouteGraphRow,
} from './route-graph-model';
import {
  adapterProfileRecoveryGuide,
  adapterStatusTextClass,
  bridgeRuntimeStatusView,
} from './adapter-view-model';

/**
 * Route detail as two zones — upstream on the left, the loopback entry on the
 * right — joined by one connector per client endpoint. Read-only apart from
 * autoStart; writing client config lives in WriteClientConfigDialog.
 * Rendered inline under the route row, not as a Dialog/popup.
 */
export function RouteDetailPanel({
  profile,
  bridgeStatus,
  statusUnavailable,
  entries,
  siblingProfiles = [],
  busy,
  error,
  onClose,
  onSetAutoStart,
  onRequestRemove,
  targetHidden = false,
}: {
  profile: AdapterProfile | null;
  bridgeStatus?: AdapterBridgeRuntimeStatus;
  statusUnavailable: boolean;
  entries: ConnectionEntry[];
  siblingProfiles?: readonly AdapterProfile[];
  busy: boolean;
  error: unknown;
  onClose: () => void;
  onSetAutoStart: (profile: AdapterProfile, autoStart: boolean) => void;
  onRequestRemove: (profile: AdapterProfile) => void;
  /** Profile's own target is hidden — disables autoStart and delete. */
  targetHidden?: boolean;
}) {
  if (!profile) return null;
  return (
    <div className="mt-3 space-y-4 border-t border-border pt-3" data-route-detail={profile.id}>
      <RouteDetailBody
        profile={profile}
        bridgeStatus={bridgeStatus}
        statusUnavailable={statusUnavailable}
        entries={entries}
        siblingProfiles={siblingProfiles}
        busy={busy}
        error={error}
        onClose={onClose}
        onSetAutoStart={onSetAutoStart}
        onRequestRemove={onRequestRemove}
        targetHidden={targetHidden}
      />
    </div>
  );
}

function RouteDetailBody({
  profile,
  bridgeStatus,
  statusUnavailable,
  entries,
  siblingProfiles,
  busy,
  error,
  onClose,
  onSetAutoStart,
  onRequestRemove,
  targetHidden,
}: {
  profile: AdapterProfile;
  bridgeStatus?: AdapterBridgeRuntimeStatus;
  statusUnavailable: boolean;
  entries: ConnectionEntry[];
  siblingProfiles: readonly AdapterProfile[];
  busy: boolean;
  error: unknown;
  onClose: () => void;
  onSetAutoStart: (profile: AdapterProfile, autoStart: boolean) => void;
  onRequestRemove: (profile: AdapterProfile) => void;
  targetHidden: boolean;
}) {
  const { toast } = useToast();
  const { t } = useI18n();
  const isBridge = profile.route === 'local_bridge';
  const endpointParts = isBridge ? adapterBridgeHostPort(profile, bridgeStatus) : null;
  const graph = buildRouteGraph({
    profile,
    entries,
    siblingProfiles,
    host: endpointParts?.host,
    port: endpointParts?.port,
  });
  const source = graph.source;
  const runtimeStatus = bridgeRuntimeStatusView({
    route: profile.route,
    bridgeState: bridgeStatus?.state,
    statusUnavailable,
  }, t);
  const recovery = adapterProfileRecoveryGuide(profile, t);
  const sourceEntry = entries.find(
    (entry) => entry.source === profile.sourceKind && entry.id === profile.sourceId,
  );
  const capabilities = readCreateRouteCapabilities(sourceEntry?.provider?.configText);
  const upstreamLabel = bridgeStatus?.upstreamStatus
    ? adapterBridgeUpstreamLabel(bridgeStatus.upstreamStatus, t)
    : null;
  const statusLine = runtimeStatus
    ? bridgeNodeStatusLine({
        runtimeLabel: runtimeStatus.label,
        upstreamLabel,
        bridgeState: bridgeStatus?.state,
        statusUnavailable,
      }, t)
    : null;
  const hostPort = endpointParts
    ? bridgeHostPortLabel({ host: endpointParts.host, port: endpointParts.port }, t)
    : null;

  return (
    <>
      <div className="min-h-0 flex-1 space-y-4 overflow-y-auto pr-1">
        <section className="grid grid-cols-1 gap-2 lg:grid-cols-[minmax(0,1fr)_auto_minmax(0,1fr)] lg:items-stretch">
          <div className={cn(
            'space-y-1.5 rounded-card border border-border bg-subtle p-3',
            source.missing && 'opacity-70',
          )}
          >
            <p className="text-meta text-muted">{t('routes.graph.upstreamTitle')}</p>
            <div className="flex min-w-0 flex-wrap items-center gap-1.5 text-sm font-medium">
              {source.agentId ? <AgentDot agentId={source.agentId} size="sm" title={null} /> : null}
              <span className="truncate">{source.title}</span>
              <Badge variant="default">{adapterCredentialKindLabel(profile.mode, t)}</Badge>
            </div>
            {source.baseUrl ? (
              <CopyableEndpoint
                text={source.baseUrl}
                url={source.baseUrl}
                ariaLabel={t('routes.graph.copyUpstream', { endpoint: source.baseUrl })}
              />
            ) : null}
            {!source.missing && source.channel !== 'unknown' ? (
              <p className="text-meta text-muted">{upstreamChannelLabel(source.channel, t)}</p>
            ) : null}
            {source.missing ? (
              <p className="text-sm text-warning">{routeSourceDeletedHint(t)}</p>
            ) : null}
          </div>

          <ZoneConnector />

          <div className="space-y-1.5 rounded-card border border-border bg-subtle p-3">
            <p className="text-meta text-muted">{t('routes.graph.localTitle')}</p>
            {graph.local.origin ? (
              <CopyableEndpoint
                text={graph.local.origin}
                url={graph.local.origin}
                ariaLabel={t('routes.graph.copyLocal', { endpoint: graph.local.origin })}
                className="text-sm font-medium"
              />
            ) : hostPort ? (
              <p className="font-mono text-sm font-medium">{hostPort}</p>
            ) : null}
            {runtimeStatus && statusLine ? (
              <p className="flex items-center gap-2 text-sm">
                <StatusPin
                  tone={runtimeStatus.tone}
                  size="md"
                  className={runtimeStatus.pulse ? 'animate-pulse' : undefined}
                />
                <span className={adapterStatusTextClass(runtimeStatus.tone)}>{statusLine.line}</span>
              </p>
            ) : null}
            {statusLine?.stoppedHint ? (
              <p className="text-meta text-muted">{statusLine.stoppedHint}</p>
            ) : null}
            {isBridge ? (
              <label className="flex items-center justify-between gap-2 pt-1 text-sm">
                <span className="min-w-0">
                  <span className="block">{t('routes.autoStart')}</span>
                  <span className="block text-xs text-muted">{t('routes.autoStartHint')}</span>
                </span>
                <Switch
                  checked={profile.autoStart}
                  disabled={busy || targetHidden}
                  aria-label={t('routes.autoStart')}
                  title={targetHidden ? t('routes.targetHiddenHint') : undefined}
                  onCheckedChange={(autoStart) => onSetAutoStart(profile, autoStart)}
                />
              </label>
            ) : null}
          </div>
        </section>

        <section className="space-y-1.5">
          <h3 className="text-body font-medium">{t('routes.graph.mappingTitle')}</h3>
          {graph.rows.length === 0 ? (
            <p className="text-sm text-muted">{t('routes.graph.empty')}</p>
          ) : (
            <>
              <div className="rounded-card border border-border bg-subtle p-3">
                <div className={cn('hidden gap-2 pb-1.5 text-meta text-muted lg:grid', MAPPING_GRID)}>
                  <span>{t('routes.graph.agentColumn')}</span>
                  <span className="text-right">{t('routes.graph.upstreamColumn')}</span>
                  <span />
                  <span>{t('routes.graph.localColumn')}</span>
                </div>
                <ul className="space-y-1.5">
                  {graph.rows.map((row) => (
                    <MappingRow key={row.agent} row={row} />
                  ))}
                </ul>
              </div>
              <p className="text-meta text-muted">{t('routes.graph.mappingHint')}</p>
            </>
          )}
          <p className="text-meta text-muted">{routeModelsSummary(capabilities.models, t)}</p>
        </section>

        {recovery ? (
          <section className="space-y-1.5" role="status">
            <h3 className="text-sm font-medium text-warning">{t('routes.recovery.stepsTitle')}</h3>
            <p className="text-sm text-secondary">{recovery.summary}</p>
            <ul className="list-disc space-y-0.5 pl-5 text-sm text-secondary">
              {recovery.steps.map((step) => <li key={step}>{step}</li>)}
            </ul>
          </section>
        ) : null}

        {error ? <AdapterErrorLines error={error} fallback={t('routes.mutationFailure')} /> : null}

        <details className="group rounded-card border border-border bg-subtle/60">
          <summary className="flex cursor-pointer list-none items-center justify-between gap-2 px-3 py-2 text-xs font-medium text-secondary marker:content-none [&::-webkit-details-marker]:hidden">
            <span className="inline-flex items-center gap-1.5">
              {t('routes.diagnostics')}
              {profile.lastErrorCode ? (
                <span className="inline-block h-1.5 w-1.5 rounded-full bg-danger" aria-hidden />
              ) : null}
            </span>
            <ChevronDown className="h-3.5 w-3.5 shrink-0 transition-transform group-open:rotate-180" aria-hidden />
          </summary>
          <div className="grid gap-1.5 border-t border-border px-3 py-3 text-xs">
            <DetailRow label={t('routes.profileId')} value={profile.id} mono />
            <DetailRow label={t('routes.rule')} value={`${profile.ruleId} · v${profile.ruleVersion}`} mono />
            {profile.lastErrorCode ? <DetailRow label={t('routes.lastError')} value={profile.lastErrorCode} mono /> : null}
            <DetailRow label={t('routes.createdAt')} value={profile.createdAt} mono />
            <DetailRow label={t('routes.updatedAt')} value={profile.updatedAt} mono />
            {source.upstreamUrls.map((url) => (
              <DetailRow key={url} label={t('routes.panel.upstreamUrl')} value={url} mono />
            ))}
            <div>
              <Button
                size="sm"
                variant="outline"
                onClick={() => {
                  void (async () => {
                    try {
                      const path = await openLogsDir();
                      toast({ title: t('routes.logsOpened'), description: path, variant: 'success' });
                    } catch (openError) {
                      toast({ title: t('routes.openFailed'), description: String(openError), variant: 'danger' });
                    }
                  })();
                }}
              >
                {t('routes.openLogs')}
              </Button>
            </div>
          </div>
        </details>
      </div>

      <div className="mt-4 flex shrink-0 flex-wrap justify-end gap-2 border-t border-border pt-4">
        <Button
          variant="dangerOutline"
          disabled={busy || targetHidden}
          title={targetHidden ? t('routes.targetHiddenHint') : undefined}
          onClick={() => onRequestRemove(profile)}
        >
          {t('routes.delete.action')}
        </Button>
        <Button variant="secondary" onClick={onClose}>{t('routes.collapse')}</Button>
      </div>
    </>
  );
}

/**
 * Connector rule. Tailwind's 1px `border-dashed` is too fine to read here, so
 * the dashes come from a gradient with an explicit 4px period. Both variants
 * inherit `currentColor` from the muted label beside them — a border-weight
 * grey disappears against the panel.
 */
const SOLID_RULE = 'h-px bg-current';
const DASHED_RULE =
  'h-px bg-[repeating-linear-gradient(to_right,currentColor_0_4px,transparent_4px_8px)]';

/** 客户端 · 上游端点 (right-aligned onto the rule) · hop rule · 本机端点 */
const MAPPING_GRID =
  'lg:grid-cols-[minmax(0,5rem)_minmax(0,1fr)_minmax(0,9rem)_minmax(0,1fr)]';

/** Solid line between the two zone cards; stacks vertically below lg. */
function ZoneConnector() {
  return (
    <div className="flex items-center justify-center text-muted lg:w-10" aria-hidden>
      <span className={cn('hidden flex-1 lg:block', SOLID_RULE)} />
      <ArrowRight className="h-4 w-4 shrink-0 rotate-90 lg:rotate-0" />
    </div>
  );
}

function MappingRow({ row }: { row: RouteGraphRow }) {
  const { t } = useI18n();
  const label = routeDetailTargetLabel(row.agent, t);
  return (
    <li className={cn(
      'grid grid-cols-1 items-center gap-1 lg:grid lg:gap-2',
      MAPPING_GRID,
      !row.enabled && 'opacity-70',
    )}
    >
      <span className="flex min-w-0 items-center gap-1.5 text-sm font-medium">
        <AgentDot agentId={row.agent} size="sm" title={null} />
        <span className="truncate">{label}</span>
      </span>
      <span className="flex min-w-0 items-center gap-1.5 lg:justify-end">
        <CopyableEndpoint
          text={row.upstreamPath || t('routes.graph.upstreamUnknown')}
          url={row.upstreamUrl}
          ariaLabel={t('routes.graph.copyUpstream', { endpoint: row.upstreamUrl || row.upstreamPath })}
        />
      </span>
      <HopLink row={row} />
      <span className="flex min-w-0 flex-wrap items-center gap-1.5">
        <CopyableEndpoint
          text={row.localPath}
          url={row.localUrl ?? ''}
          ariaLabel={t('routes.graph.copyLocal', { endpoint: row.localUrl ?? row.localPath })}
        />
        {row.applied ? (
          <span className="shrink-0 text-meta text-success">{t('routes.graph.applied')}</span>
        ) : null}
        {!row.enabled ? (
          <span className="shrink-0 text-meta text-muted">{t('routes.graph.notEnabled')}</span>
        ) : null}
      </span>
    </li>
  );
}

/** Dashed when AgentHub rewrites the protocol, solid when it passes through. */
function HopLink({ row }: { row: RouteGraphRow }) {
  const { t } = useI18n();
  const line = cn(
    'hidden min-w-2 flex-1 lg:block',
    row.link === 'dashed' ? DASHED_RULE : SOLID_RULE,
  );
  return (
    <span className="flex items-center gap-1 text-muted" data-hop-link={row.link}>
      <span className={line} aria-hidden />
      <span className="shrink-0 text-meta">{routeGraphLinkLabel(row.hop, t)}</span>
      <span className={line} aria-hidden />
      <ArrowRight className="h-3 w-3 shrink-0" aria-hidden />
    </span>
  );
}

/** Shows a short path or origin, copies the full endpoint. */
function CopyableEndpoint({
  text,
  url,
  ariaLabel,
  className,
}: {
  text: string;
  url: string;
  ariaLabel: string;
  className?: string;
}) {
  const { t } = useI18n();
  const { toast } = useToast();
  const canCopy = Boolean(url.trim());
  return (
    <Hint label={canCopy ? url : routeCopyPortPendingLabel(t)}>
      <button
        type="button"
        className="inline-flex max-w-full items-center gap-1 rounded-btn px-1 py-0.5 text-left hover:bg-hover disabled:cursor-not-allowed disabled:opacity-60"
        disabled={!canCopy}
        aria-label={ariaLabel}
        onClick={() => {
          if (!canCopy) return;
          void (async () => {
            try {
              await navigator.clipboard.writeText(url);
              toast({ title: t('routes.endpointCopied'), description: url });
            } catch {
              toast({ title: t('routes.copyFailed'), variant: 'danger' });
            }
          })();
        }}
      >
        <span className={cn('truncate font-mono text-xs text-secondary', className)}>{text}</span>
        <Copy className="h-3 w-3 shrink-0 text-muted" aria-hidden />
      </button>
    </Hint>
  );
}
