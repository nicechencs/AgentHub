import { useEffect, useMemo, useState } from 'react';
import { ArrowRight, ChevronDown, Copy } from 'lucide-react';
import { AgentDot } from '@/components/shared/AgentDot';
import { DetailRow } from '@/components/shared/DetailRow';
import { useI18n } from '@/components/shared/LanguageProvider';
import { StatusPin } from '@/components/shared/StatusPin';
import { RouteEndpointUrl } from '@/components/shared/RouteEndpointUrl';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Switch } from '@/components/ui/switch';
import { useToast } from '@/components/ui/toast';
import { openLogsDir } from '@/lib/api/settings';
import { formatRouteEndpointHttpUrl } from '@/lib/route-endpoints';
import type {
  AdapterBridgeRuntimeStatus,
  AdapterProfile,
} from '@/lib/backend/contracts/adapter';
import type { TicketSurfaceGroupView } from '@/lib/backend/contracts/ticket';
import type { ConnectionEntry } from '@/lib/connection-entry';
import { cn } from '@/lib/utils';
import { AdapterErrorLines } from './adapter-components';
import { readCreateRouteCapabilities, type CreateRouteTarget } from './create-route-flow';
import { bridgeMemberRows, memberPinTone } from './adapter-member-model';
import {
  adapterBridgeHostPort,
  adapterBridgeUpstreamLabel,
  adapterCredentialKindLabel,
} from './adapter-model';
import {
  bridgeHostPortLabel,
  bridgeNodeStatusLine,
  buildRouteDetailEdges,
  buildRouteDetailSourceView,
  defaultApplySelection,
  routeCopyPortPendingLabel,
  routeDetailApplyConfirmLabel,
  routeDetailTargetLabel,
  routeEdgeSupportLabel,
  routeHopLabel,
  routeModelsSummary,
  routeSourceDeletedHint,
  selectableProductTargets,
  upstreamChannelLabel,
  type RouteDetailEdgeTarget,
  type RouteDetailEdgeView,
  type RouteEdgeSupport,
} from './adapter-route-detail-model';
import {
  adapterProfileRecoveryGuide,
  adapterStatusTextClass,
  bridgeRuntimeStatusView,
} from './adapter-view-model';

/**
 * Read-only runtime detail redesigned as a source → bridge → clients graph.
 * AutoStart is the only editable field; Quick Apply checkboxes live on edges.
 * Rendered inline under the route row — not a Dialog/popup.
 */
export function AdapterProfileDetailDialog({
  profile,
  bridgeStatus,
  statusUnavailable,
  entries,
  siblingProfiles = [],
  surfaceGroups = [],
  busy,
  error,
  onClose,
  onSetAutoStart,
  onRequestRemove,
  onApplyRoute,
  hiddenTargetIds,
  targetHidden = false,
}: {
  profile: AdapterProfile | null;
  bridgeStatus?: AdapterBridgeRuntimeStatus;
  statusUnavailable: boolean;
  entries: ConnectionEntry[];
  siblingProfiles?: readonly AdapterProfile[];
  surfaceGroups?: readonly TicketSurfaceGroupView[];
  busy: boolean;
  error: unknown;
  onClose: () => void;
  onSetAutoStart: (profile: AdapterProfile, autoStart: boolean) => void;
  onRequestRemove: (profile: AdapterProfile) => void;
  onApplyRoute?: (profile: AdapterProfile, agents: readonly CreateRouteTarget[]) => void;
  hiddenTargetIds?: ReadonlySet<string>;
  /** Profile's own target is hidden — disables autoStart / unbind like before. */
  targetHidden?: boolean;
}) {
  if (!profile) return null;
  return (
    <div className="mt-3 space-y-4 border-t border-border pt-3" data-route-detail={profile.id}>
      <ProfileDetailBody
        profile={profile}
        bridgeStatus={bridgeStatus}
        statusUnavailable={statusUnavailable}
        entries={entries}
        siblingProfiles={siblingProfiles}
        surfaceGroups={surfaceGroups}
        busy={busy}
        error={error}
        onClose={onClose}
        onSetAutoStart={onSetAutoStart}
        onRequestRemove={onRequestRemove}
        onApplyRoute={onApplyRoute}
        hiddenTargetIds={hiddenTargetIds}
        targetHidden={targetHidden}
      />
    </div>
  );
}

function ProfileDetailBody({
  profile,
  bridgeStatus,
  statusUnavailable,
  entries,
  siblingProfiles,
  surfaceGroups,
  busy,
  error,
  onClose,
  onSetAutoStart,
  onRequestRemove,
  onApplyRoute,
  hiddenTargetIds,
  targetHidden,
}: {
  profile: AdapterProfile;
  bridgeStatus?: AdapterBridgeRuntimeStatus;
  statusUnavailable: boolean;
  entries: ConnectionEntry[];
  siblingProfiles: readonly AdapterProfile[];
  surfaceGroups: readonly TicketSurfaceGroupView[];
  busy: boolean;
  error: unknown;
  onClose: () => void;
  onSetAutoStart: (profile: AdapterProfile, autoStart: boolean) => void;
  onRequestRemove: (profile: AdapterProfile) => void;
  onApplyRoute?: (profile: AdapterProfile, agents: readonly CreateRouteTarget[]) => void;
  hiddenTargetIds?: ReadonlySet<string>;
  targetHidden: boolean;
}) {
  const { toast } = useToast();
  const { t } = useI18n();
  const source = buildRouteDetailSourceView({ profile, entries });
  const runtimeStatus = bridgeRuntimeStatusView({
    route: profile.route,
    bridgeState: bridgeStatus?.state,
    statusUnavailable,
  }, t);
  const isBridge = profile.route === 'local_bridge';
  const endpointParts = isBridge ? adapterBridgeHostPort(profile, bridgeStatus) : null;
  const recovery = adapterProfileRecoveryGuide(profile, t);
  const members = isBridge
    ? bridgeMemberRows({ profile, groups: surfaceGroups, entries, t })
    : [];
  const sourceEntry = entries.find(
    (entry) => entry.source === profile.sourceKind && entry.id === profile.sourceId,
  );
  const capabilities = readCreateRouteCapabilities(sourceEntry?.provider?.configText);
  const edges = useMemo(
    () => (isBridge
      ? buildRouteDetailEdges({
          profile,
          entries,
          siblingProfiles,
          hiddenTargetIds,
        })
      : []),
    [isBridge, profile, entries, siblingProfiles, hiddenTargetIds],
  );
  const [applyTargets, setApplyTargets] = useState<RouteDetailEdgeTarget[]>(
    () => defaultApplySelection(edges),
  );
  useEffect(() => {
    setApplyTargets(defaultApplySelection(edges));
  }, [edges]);

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
  const selectedProducts = selectableProductTargets(applyTargets);
  const writeDisabled = busy
    || targetHidden
    || source.missing
    || selectedProducts.length === 0;

  const truncateUrl = (url: string) => (
    url.length > 42 ? `${url.slice(0, 40)}…` : url
  );

  return (
    <>
      <div className="min-h-0 flex-1 space-y-4 overflow-y-auto pr-1">
        <section className="grid grid-cols-1 gap-3 lg:grid-cols-[minmax(0,1fr)_auto_minmax(0,1fr)_auto_minmax(0,1.35fr)] lg:items-stretch">
          <div className={cn(
            'space-y-1.5 rounded-card border border-border bg-subtle p-3',
            source.missing && 'opacity-70',
          )}
          >
            <p className="text-meta text-muted">{t('routes.panel.sourceTitle')}</p>
            <div className="flex min-w-0 flex-wrap items-center gap-1.5 text-sm font-medium">
              {source.agentId ? <AgentDot agentId={source.agentId} size="sm" title={null} /> : null}
              <span className="truncate">{source.title}</span>
            </div>
            <Badge variant="default">{adapterCredentialKindLabel(profile.mode, t)}</Badge>
            {!source.missing && source.channel !== 'unknown' ? (
              <p className="text-meta text-muted">{upstreamChannelLabel(source.channel, t)}</p>
            ) : null}
            {!source.missing && source.baseUrl ? (
              <p className="truncate font-mono text-meta text-secondary" title={source.baseUrl}>
                {truncateUrl(source.baseUrl)}
              </p>
            ) : null}
            {source.missing ? (
              <p className="text-sm text-warning">{routeSourceDeletedHint(t)}</p>
            ) : null}
          </div>

          <div className="hidden items-center justify-center lg:flex" aria-hidden>
            <ArrowRight className="h-4 w-4 text-muted" />
          </div>

          <div className="space-y-1.5 rounded-card border border-border bg-subtle p-3">
            <p className="text-meta text-muted">{t('routes.panel.bridgeTitle')}</p>
            {hostPort ? (
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

          <div className="hidden items-center justify-center lg:flex" aria-hidden>
            <ArrowRight className="h-4 w-4 text-muted" />
          </div>

          <div className="space-y-2 rounded-card border border-border bg-subtle p-3">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <p className="text-sm font-medium">{t('routes.panel.clientsTitle')}</p>
              <Button
                size="sm"
                disabled={writeDisabled}
                title={targetHidden ? t('routes.targetHiddenHint') : undefined}
                onClick={() => onApplyRoute?.(profile, selectedProducts)}
              >
                {routeDetailApplyConfirmLabel(t)}
              </Button>
            </div>
            <ul className="space-y-2">
              {edges.map((edge) => (
                <EdgeRow
                  key={edge.target}
                  edge={edge}
                  port={endpointParts?.port}
                  host={endpointParts?.host}
                  checked={applyTargets.includes(edge.target)}
                  onToggle={() => {
                    if (!edge.selectable) return;
                    setApplyTargets((current) => (
                      current.includes(edge.target)
                        ? current.filter((item) => item !== edge.target)
                        : [...current, edge.target]
                    ));
                  }}
                  disabledWrite={busy || targetHidden || source.missing}
                />
              ))}
            </ul>
            <p className="text-meta text-muted">{routeModelsSummary(capabilities.models, t)}</p>
          </div>
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

        {members.length >= 2 ? (
          <section className="space-y-1.5">
            <h3 className="text-body font-medium">{t('routes.members.title')}</h3>
            <ul className="space-y-1.5 rounded-card border border-border bg-subtle p-3">
              {members.map((member) => (
                <li
                  key={member.ticketId}
                  className={cn(
                    'flex min-w-0 items-start gap-2',
                    member.isolated && 'text-muted',
                  )}
                >
                  <StatusPin
                    tone={memberPinTone(member)}
                    size="md"
                    className="mt-1.5"
                  />
                  <div className="min-w-0 flex-1">
                    <div className="flex min-w-0 flex-wrap items-center gap-1.5">
                      {member.agentId ? (
                        <AgentDot agentId={member.agentId} size="sm" title={null} />
                      ) : null}
                      <span className={cn('truncate text-body', member.isolated ? 'text-muted' : 'text-primary')}>
                        {member.label}
                      </span>
                      {member.lead ? (
                        <Badge variant="default">{t('routes.members.lead')}</Badge>
                      ) : null}
                    </div>
                    <p className="text-meta text-muted">{member.reason}</p>
                  </div>
                </li>
              ))}
            </ul>
          </section>
        ) : null}

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
          {t('routes.unbind.action')}
        </Button>
        <Button variant="secondary" onClick={onClose}>{t('routes.close')}</Button>
      </div>
    </>
  );
}

function EdgeRow({
  edge,
  port,
  host,
  checked,
  onToggle,
  disabledWrite,
}: {
  edge: RouteDetailEdgeView;
  port?: number | null;
  host?: string;
  checked: boolean;
  onToggle: () => void;
  disabledWrite: boolean;
}) {
  const { t } = useI18n();
  const { toast } = useToast();
  const label = routeDetailTargetLabel(edge.target, t);
  const muted = edge.support === 'source_missing'
    || edge.support === 'hidden'
    || edge.support === 'no_upstream';
  const href = formatRouteEndpointHttpUrl({
    path: edge.path,
    port,
    host,
  });
  const canCopy = Boolean(href && port);
  const mark = edgeSupportMark(edge.support);

  return (
    <li className={cn('space-y-0.5', muted && 'opacity-70')}>
      <div className="flex min-w-0 flex-wrap items-center gap-2">
        {edge.selectable ? (
          <label className="inline-flex items-center gap-1.5">
            <input
              type="checkbox"
              checked={checked}
              disabled={disabledWrite}
              onChange={onToggle}
            />
            <span className="text-sm font-medium">{mark} {label}</span>
          </label>
        ) : (
          <span className="text-sm font-medium">{mark} {label}</span>
        )}
        <button
          type="button"
          className="inline-flex max-w-full items-center gap-1 rounded-btn px-1 py-0.5 text-left hover:bg-hover disabled:cursor-not-allowed disabled:opacity-60"
          disabled={!canCopy}
          title={!canCopy ? routeCopyPortPendingLabel(t) : undefined}
          aria-label={t('routes.copyEndpointAria', { endpoint: href ?? edge.path })}
          onClick={() => {
            if (!href || !canCopy) return;
            void (async () => {
              try {
                await navigator.clipboard.writeText(href);
                toast({ title: t('routes.endpointCopied'), description: href });
              } catch {
                toast({ title: t('routes.copyFailed'), variant: 'danger' });
              }
            })();
          }}
        >
          <RouteEndpointUrl
            path={edge.path}
            port={port}
            host={host}
            endpointId={edge.endpointId}
            className="text-xs"
          />
          <Copy className="h-3 w-3 shrink-0 text-muted" aria-hidden />
        </button>
      </div>
      <p className="text-meta text-muted">{routeHopLabel(edge.hop, edge.upstreamChannel, t)}</p>
      <p className="text-meta text-secondary">{routeEdgeSupportLabel(edge.support, label, t)}</p>
      {edge.upstreamUrl ? (
        <p className="truncate font-mono text-meta text-muted" title={edge.upstreamUrl}>
          {edge.upstreamUrl}
        </p>
      ) : null}
    </li>
  );
}

function edgeSupportMark(support: RouteEdgeSupport): string {
  if (support === 'applied') return '✓';
  if (support === 'ready') return '○';
  return '·';
}
