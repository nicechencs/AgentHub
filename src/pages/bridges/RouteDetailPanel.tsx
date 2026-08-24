import { ChevronDown, Copy } from 'lucide-react';
import { AgentDot } from '@/components/shared/AgentDot';
import { DetailRow } from '@/components/shared/DetailRow';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
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
  adapterCredentialKindLabel,
} from './adapter-model';
import {
  routeCopyPortPendingLabel,
  routeDetailTargetLabel,
  routeModelsSummary,
  routeSourceDeletedHint,
} from './adapter-route-detail-model';
import {
  buildRouteGraph,
  groupRouteGraphRowsByUpstream,
  routeGraphLinkLabel,
  routeGraphSharesUpstreamEndpoint,
  type RouteGraphRow,
  type RouteMappingGroup,
} from './route-graph-model';
import {
  adapterProfileRecoveryGuide,
} from './adapter-view-model';

/**
 * Route detail: endpoint mapping with upstream/local URLs merged in, plus
 * autoStart in the footer. Writing client config lives in WriteClientConfigDialog.
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
  _onSetAutoStart,
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
        onSetAutoStart={_onSetAutoStart}
        onRequestRemove={onRequestRemove}
        targetHidden={targetHidden}
      />
    </div>
  );
}

function RouteDetailBody({
  profile,
  bridgeStatus,
  statusUnavailable: _statusUnavailable,
  entries,
  siblingProfiles,
  busy,
  error,
  onClose,
  _onSetAutoStart,
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
  const recovery = adapterProfileRecoveryGuide(profile, t);
  const sourceEntry = entries.find(
    (entry) => entry.source === profile.sourceKind && entry.id === profile.sourceId,
  );
  const capabilities = readCreateRouteCapabilities(sourceEntry?.provider?.configText);
  const mappingGroups = groupRouteGraphRowsByUpstream(graph.rows);
  const sharesUpstream = routeGraphSharesUpstreamEndpoint(graph.rows);

  return (
    <>
      <div className="min-h-0 flex-1 space-y-4 overflow-y-auto pr-1">
        <section className="space-y-2">
          <h3 className="text-body font-medium">{t('routes.graph.mappingTitle')}</h3>
          <div className={cn(
            'rounded-card border border-border bg-subtle p-3 space-y-2',
            source.missing && 'opacity-70',
          )}
          >
            <div className="flex min-w-0 flex-wrap items-center gap-1.5 text-sm font-medium">
              {source.agentId ? <AgentDot agentId={source.agentId} size="sm" title={null} /> : null}
              <span className="truncate">{source.title}</span>
              <Badge variant="default">{adapterCredentialKindLabel(profile.mode, t)}</Badge>
            </div>
            {source.missing ? (
              <p className="text-sm text-warning">{routeSourceDeletedHint(t)}</p>
            ) : null}

            <div className="flex flex-col gap-1.5 sm:flex-row sm:flex-wrap sm:items-center sm:gap-x-4 sm:gap-y-1">
              {source.baseUrl ? (
                <span className="flex min-w-0 items-center gap-1.5 text-xs">
                  <span className="shrink-0 text-muted">{t('routes.graph.upstreamBase')}</span>
                  <CopyableEndpoint
                    text={source.baseUrl}
                    url={source.baseUrl}
                    ariaLabel={t('routes.graph.copyUpstream', { endpoint: source.baseUrl })}
                  />
                </span>
              ) : null}
              {graph.local.origin ? (
                <span className="flex min-w-0 items-center gap-1.5 text-xs">
                  <span className="shrink-0 text-muted">{t('routes.graph.localBase')}</span>
                  <CopyableEndpoint
                    text={graph.local.origin}
                    url={graph.local.origin}
                    ariaLabel={t('routes.graph.copyLocal', { endpoint: graph.local.origin })}
                    className="text-sm font-medium"
                  />
                </span>
              ) : null}
            </div>

            {graph.rows.length === 0 ? (
              <p className="text-sm text-muted">{t('routes.graph.empty')}</p>
            ) : sharesUpstream && graph.rows[0]?.upstreamPath ? (
              <MappingTableSharedUpstream
                upstreamPath={graph.rows[0].upstreamPath}
                upstreamUrl={graph.rows[0].upstreamUrl}
                rows={graph.rows}
              />
            ) : (
              <div className="space-y-3">
                {mappingGroups.map((group) => (
                  <MappingTableGroup key={`${group.upstreamBaseUrl}:${group.upstreamPath}`} group={group} />
                ))}
              </div>
            )}
          </div>
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

      <div className="mt-4 flex shrink-0 flex-wrap items-center justify-end gap-3 border-t border-border pt-4">
        <div className="flex shrink-0 flex-wrap justify-end gap-2">
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
      </div>
    </>
  );
}

const MAPPING_GRID = 'lg:grid-cols-[minmax(0,5rem)_minmax(0,7rem)_minmax(0,1fr)]';
const MAPPING_GRID_WITH_UPSTREAM = 'lg:grid-cols-[minmax(0,5rem)_minmax(0,1fr)_minmax(0,7rem)_minmax(0,1fr)]';

const SOLID_RULE = 'h-px bg-current';
const DASHED_RULE =
  'h-px bg-[repeating-linear-gradient(to_right,currentColor_0_4px,transparent_4px_8px)]';

function MappingTableSharedUpstream({
  upstreamPath,
  upstreamUrl,
  rows,
}: {
  upstreamPath: string;
  upstreamUrl: string;
  rows: readonly RouteGraphRow[];
}) {
  const { t } = useI18n();
  return (
    <div className="space-y-1.5">
      <div className="flex min-w-0 flex-wrap items-center gap-1.5 text-xs">
        <span className="shrink-0 text-muted">{t('routes.graph.upstreamColumn')}</span>
        <CopyableEndpoint
          text={upstreamPath}
          url={upstreamUrl}
          ariaLabel={t('routes.graph.copyUpstream', { endpoint: upstreamUrl || upstreamPath })}
        />
      </div>
      <div className={cn('hidden gap-2 pb-1.5 text-meta text-muted lg:grid', MAPPING_GRID)}>
        <span>{t('routes.graph.agentColumn')}</span>
        <span>{t('routes.graph.convertColumn')}</span>
        <span>{t('routes.graph.localColumn')}</span>
      </div>
      <ul className="space-y-1.5">
        {rows.map((row) => (
          <MappingRow key={row.agent} row={row} showUpstream={false} />
        ))}
      </ul>
    </div>
  );
}

function MappingTableGroup({ group }: { group: RouteMappingGroup }) {
  const { t } = useI18n();
  return (
    <div className="space-y-1.5">
      {group.upstreamPath ? (
        <div className="flex min-w-0 flex-wrap items-center gap-1.5 text-xs">
          <span className="shrink-0 text-muted">{t('routes.graph.upstreamColumn')}</span>
          <CopyableEndpoint
            text={group.upstreamPath}
            url={group.upstreamUrl}
            ariaLabel={t('routes.graph.copyUpstream', { endpoint: group.upstreamUrl || group.upstreamPath })}
          />
        </div>
      ) : null}
      <div className={cn('hidden gap-2 pb-1.5 text-meta text-muted lg:grid', MAPPING_GRID)}>
        <span>{t('routes.graph.agentColumn')}</span>
        <span>{t('routes.graph.convertColumn')}</span>
        <span>{t('routes.graph.localColumn')}</span>
      </div>
      <ul className="space-y-1.5">
        {group.rows.map((row) => (
          <MappingRow key={row.agent} row={row} showUpstream={false} />
        ))}
      </ul>
    </div>
  );
}

function MappingRow({ row, showUpstream }: { row: RouteGraphRow; showUpstream: boolean }) {
  const { t } = useI18n();
  const label = routeDetailTargetLabel(row.agent, t);
  const wireNote = clientWireNote(row.agent, t);
  const grid = showUpstream ? MAPPING_GRID_WITH_UPSTREAM : MAPPING_GRID;
  return (
    <li className={cn(
      'grid grid-cols-1 items-center gap-1 lg:grid lg:gap-2',
      grid,
      !row.enabled && 'opacity-70',
    )}
    >
      <span className="flex min-w-0 flex-col gap-0.5">
        <span className="flex min-w-0 items-center gap-1.5 text-sm font-medium">
          <AgentDot agentId={row.agent} size="sm" title={null} />
          <span className="truncate">{label}</span>
        </span>
        {wireNote ? <span className="pl-5 text-meta text-muted">{wireNote}</span> : null}
      </span>
      {showUpstream ? (
        <span className="flex min-w-0 items-center gap-1.5 lg:justify-end">
          <CopyableEndpoint
            text={row.upstreamPath || t('routes.graph.upstreamUnknown')}
            url={row.upstreamUrl}
            ariaLabel={t('routes.graph.copyUpstream', { endpoint: row.upstreamUrl || row.upstreamPath })}
          />
        </span>
      ) : null}
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

function clientWireNote(agent: RouteGraphRow['agent'], t: ReturnType<typeof useI18n>['t']): string | null {
  if (agent === 'codex') return t('routes.write.wireNote.codex');
  if (agent === 'grok') return t('routes.write.wireNote.grok');
  if (agent === 'claude') return t('routes.write.wireNote.claude');
  return null;
}

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
    </span>
  );
}

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
