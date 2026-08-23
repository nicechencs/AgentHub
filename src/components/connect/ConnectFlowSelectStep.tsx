import { KeyRound, RefreshCw, Wallet } from 'lucide-react';
import { AgentDot } from '@/components/shared/AgentDot';
import { AgentLogo } from '@/components/shared/AgentLogo';
import { CurrentBadge } from '@/components/shared/CurrentBadge';
import { EmptyState } from '@/components/shared/EmptyState';
import { useI18n } from '@/components/shared/LanguageProvider';
import { ListRow } from '@/components/shared/ListRow';
import { Notice } from '@/components/shared/Notice';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Skeleton } from '@/components/ui/skeleton';
import { RouteEndpointUrl } from '@/components/shared/RouteEndpointUrl';
import { agentDisplayName } from '@/config/agents';
import { planMaturityLabel, planRouteSummary } from '@/lib/connect-flow/eligibility';
import { ROUTE_ENDPOINTS } from '@/lib/route-endpoints';
import type { AgentId } from '@/lib/types';
import type {
  ConnectFlowEntry,
  PlanEligibility,
  SourceOption,
} from '@/lib/connect-flow/types';
import { cn } from '@/lib/utils';
import {
  agentsForRouteEndpoint,
  eligibilityForRouteEndpoint,
  eligibilityOf,
  isOptionSelectable,
  isTargetSelectable,
  planEligibilityAllowsApply,
  representativeAgentForRouteEndpoint,
  resolveEmptyKind,
  shouldShowConnectGuideActions,
  shouldShowSelectSkeleton,
  splitSourceOptions,
  type ConnectFlowState,
} from './connect-flow-state';

export function EffectiveSummary({
  agentId,
  label,
  authLabel,
}: {
  agentId: AgentId;
  label: string;
  authLabel: string;
}) {
  const { t } = useI18n();
  return (
    <span className="flex flex-wrap items-center gap-1.5">
      <AgentDot agentId={agentId} size="sm" title={null} />
      <span>{t('connect.dialog.currentEffective', { label })}</span>
      <Badge variant="default">{authLabel}</Badge>
    </span>
  );
}

export function FixedSourceSummary({
  entry,
  accounts,
  providers,
}: {
  entry: Extract<ConnectFlowEntry, { mode: 'for-source' }>;
  accounts: { id: string; label: string; agentId: AgentId }[];
  providers: { id: string; name: string; agentId: AgentId }[];
}) {
  const { t } = useI18n();
  const record = entry.source.kind === 'account'
    ? accounts.find((item) => item.id === entry.source.id)
    : providers.find((item) => item.id === entry.source.id);
  const label = record
    ? ('label' in record ? record.label : record.name)
    : `${entry.source.kind}:${entry.source.id}`;
  const agentId = record?.agentId;
  const attach = entry.purpose === 'route'
    ? t('connect.dialog.attachRoute', { label })
    : entry.purpose === 'share'
      ? t('connect.dialog.attachShare', { label })
      : t('connect.dialog.attachToOthers', { label });
  return (
    <span className="flex flex-wrap items-center gap-1.5">
      {agentId ? <AgentDot agentId={agentId} size="sm" title={null} /> : null}
      <span>{attach}</span>
    </span>
  );
}

export function SelectLoadingSkeleton() {
  return (
    <div className="space-y-2">
      <Skeleton className="h-12 w-full" />
      <Skeleton className="h-12 w-full" />
      <Skeleton className="h-12 w-full" />
    </div>
  );
}

export function ConnectFlowSelectStep({
  entry,
  state,
  options,
  eligibilities,
  targetAgentIds,
  sourceAgentId,
  emptyKind,
  poolLoading,
  profilesReady,
  onSelectSource,
  onSelectTarget,
  onRetryEligibility,
  onRetryResources,
  onGoImport,
  onGoNewKey,
  onOauthGuide,
}: {
  entry: ConnectFlowEntry;
  state: ConnectFlowState;
  options: SourceOption[];
  eligibilities: ReadonlyMap<string, PlanEligibility>;
  targetAgentIds: AgentId[];
  sourceAgentId: AgentId | null;
  emptyKind: ReturnType<typeof resolveEmptyKind>;
  poolLoading: boolean;
  profilesReady: boolean;
  onSelectSource: (option: SourceOption) => void;
  onSelectTarget: (agentId: AgentId) => void;
  onRetryEligibility: (request: { source: SourceOption['ref']; targetAgentId: AgentId }) => void;
  onRetryResources: () => void;
  onGoImport: () => void;
  onGoNewKey: () => void;
  onOauthGuide: (agentId: AgentId) => void;
}) {
  const { t } = useI18n();
  if (emptyKind.kind === 'partial_load_error') {
    return (
      <div className="space-y-3">
        <Notice tone="danger" actionLabel={t('chrome.error.retry')} onAction={onRetryResources}>
          {emptyKind.message}
        </Notice>
        {entry.mode === 'for-agent' && options.length > 0 ? (
          <SourceGroups
            options={options}
            state={state}
            eligibilities={eligibilities}
            targetAgentId={entry.targetAgentId}
            onSelectSource={onSelectSource}
            onRetryEligibility={onRetryEligibility}
            onOauthGuide={onOauthGuide}
          />
        ) : null}
      </div>
    );
  }

  if (emptyKind.kind === 'preset_invalid' || emptyKind.kind === 'preset_deleted') {
    return (
      <Notice tone="danger">{emptyKind.message}</Notice>
    );
  }

  if (shouldShowSelectSkeleton({
    profilesReady,
    poolLoading,
    optionsLength: options.length,
    targetAgentIdsLength: targetAgentIds.length,
  })) {
    return <SelectLoadingSkeleton />;
  }

  if (emptyKind.kind === 'wallet_empty') {
    return (
      <EmptyState
        icon={Wallet}
        title={t('connect.select.emptyTitle')}
        description={t('connect.select.emptyDesc')}
        action={
          <Button size="sm" variant="outline" className="mt-2" onClick={onGoImport}>
            {t('connect.select.emptyAction')}
          </Button>
        }
      />
    );
  }

  return (
    <div className="space-y-4">
      {entry.mode === 'for-agent' ? (
        <SourceGroups
          options={options}
          state={state}
          eligibilities={eligibilities}
          targetAgentId={entry.targetAgentId}
          onSelectSource={onSelectSource}
          onRetryEligibility={onRetryEligibility}
          onOauthGuide={onOauthGuide}
        />
      ) : entry.purpose === 'route' ? (
        <EndpointGrid
          targetAgentIds={targetAgentIds}
          selected={state.selectedTargetAgentId}
          source={entry.source}
          sourceAgentId={sourceAgentId}
          eligibilities={eligibilities}
          onSelect={onSelectTarget}
          onRetryEligibility={onRetryEligibility}
          onOauthGuide={onOauthGuide}
        />
      ) : (
        <TargetGrid
          targetAgentIds={targetAgentIds}
          selected={state.selectedTargetAgentId}
          source={entry.source}
          sourceAgentId={sourceAgentId}
          eligibilities={eligibilities}
          onSelect={onSelectTarget}
          onRetryEligibility={onRetryEligibility}
          onOauthGuide={onOauthGuide}
        />
      )}

      {emptyKind.kind === 'all_infeasible' ? (
        <Notice tone="warning">
          {entry.mode === 'for-source'
            ? entry.purpose === 'share'
              ? t('connect.select.allInfeasibleShare')
              : entry.purpose === 'route'
                ? t('connect.select.allInfeasibleRoute')
                : t('connect.select.allInfeasibleSource')
            : t('connect.select.allInfeasibleAgent')}
        </Notice>
      ) : null}

      {shouldShowConnectGuideActions(entry) ? (
        <GuideActions onGoImport={onGoImport} onGoNewKey={onGoNewKey} />
      ) : null}
    </div>
  );
}

function SourceGroups({
  options,
  state,
  eligibilities,
  targetAgentId,
  onSelectSource,
  onRetryEligibility,
  onOauthGuide,
}: {
  options: SourceOption[];
  state: ConnectFlowState;
  eligibilities: ReadonlyMap<string, PlanEligibility>;
  targetAgentId: AgentId;
  onSelectSource: (option: SourceOption) => void;
  onRetryEligibility: (request: { source: SourceOption['ref']; targetAgentId: AgentId }) => void;
  onOauthGuide: (agentId: AgentId) => void;
}) {
  const { t } = useI18n();
  const { native, cross } = splitSourceOptions(options);
  return (
    <div className="space-y-4">
      <section className="space-y-2">
        <h3 className="text-sm font-medium">{t('connect.select.nativeTitle')}</h3>
        {native.length === 0 ? (
          <p className="text-xs text-muted">{t('connect.select.nativeEmpty')}</p>
        ) : native.map((item) => (
          <NativeOptionRow
            key={`${item.ref.kind}:${item.ref.id}`}
            option={item}
            active={state.selectedSource?.kind === item.ref.kind && state.selectedSource.id === item.ref.id}
            onSelect={onSelectSource}
          />
        ))}
      </section>
      <section className="space-y-2">
        <h3 className="text-sm font-medium">{t('connect.select.crossTitle')}</h3>
        {cross.length === 0 ? (
          <p className="text-xs text-muted">{t('connect.select.crossEmpty')}</p>
        ) : cross.map((item) => (
          <CrossOptionRow
            key={`${item.ref.kind}:${item.ref.id}`}
            option={item}
            eligibility={eligibilityOf(eligibilities, item.ref, targetAgentId)}
            active={state.selectedSource?.kind === item.ref.kind && state.selectedSource.id === item.ref.id}
            onSelect={onSelectSource}
            onRetry={() => onRetryEligibility({ source: item.ref, targetAgentId })}
            onOauthGuide={() => onOauthGuide(item.agentId)}
          />
        ))}
      </section>
    </div>
  );
}

function NativeOptionRow({
  option,
  active,
  onSelect,
}: {
  option: SourceOption;
  active: boolean;
  onSelect: (option: SourceOption) => void;
}) {
  const { t } = useI18n();
  const disabled = option.state.kind === 'current' || option.state.kind === 'blocked_native';
  return (
    <ListRow
      active={active}
      className={cn('px-3 py-2', disabled && 'opacity-60')}
      role="button"
      tabIndex={disabled ? -1 : 0}
      aria-disabled={disabled}
      onClick={() => {
        if (!disabled) onSelect(option);
      }}
      onKeyDown={(event) => {
        if (disabled) return;
        if (event.key === 'Enter' || event.key === ' ') {
          event.preventDefault();
          onSelect(option);
        }
      }}
    >
      <div className="flex min-w-0 items-start justify-between gap-2">
        <div className="min-w-0">
          <p className="truncate text-sm font-medium">{option.label}</p>
          {option.sublabel ? <p className="truncate text-xs text-muted">{option.sublabel}</p> : null}
          {option.viaAdapter ? (
            <p className="mt-0.5 text-xs text-secondary">
              {t('connect.select.viaSource', { source: option.viaAdapter.sourceLabel })}
            </p>
          ) : null}
          {option.state.kind === 'blocked_native' ? (
            <p className="mt-0.5 text-xs text-warning">{option.state.reason}</p>
          ) : null}
        </div>
        {option.state.kind === 'current' ? (
          <span className="flex items-center gap-1">
            <CurrentBadge />
            <span className="text-xs text-secondary">{t('connect.select.currentlyUsed')}</span>
          </span>
        ) : null}
      </div>
    </ListRow>
  );
}

function CrossOptionRow({
  option,
  eligibility,
  active,
  onSelect,
  onRetry,
  onOauthGuide,
}: {
  option: SourceOption;
  eligibility: PlanEligibility | undefined;
  active: boolean;
  onSelect: (option: SourceOption) => void;
  onRetry: () => void;
  onOauthGuide: () => void;
}) {
  const selectable = isOptionSelectable(option, eligibility);
  const disabled = !selectable;
  return (
    <ListRow
      active={active && selectable}
      className={cn('px-3 py-2', disabled && eligibility?.kind !== 'loading' && 'opacity-60')}
      role="button"
      tabIndex={disabled ? -1 : 0}
      aria-disabled={disabled}
      onClick={() => {
        if (selectable) onSelect(option);
      }}
      onKeyDown={(event) => {
        if (!selectable) return;
        if (event.key === 'Enter' || event.key === ' ') {
          event.preventDefault();
          onSelect(option);
        }
      }}
    >
      <div className="flex min-w-0 items-start justify-between gap-2">
        <div className="min-w-0 flex-1">
          <p className="flex items-center gap-1.5 truncate text-sm font-medium">
            <AgentDot agentId={option.agentId} size="sm" title={null} />
            {option.label}
          </p>
          {option.sublabel ? <p className="truncate text-xs text-muted">{option.sublabel}</p> : null}
          <EligibilityBody
            eligibility={eligibility}
            onRetry={onRetry}
            onOauthGuide={onOauthGuide}
          />
        </div>
      </div>
    </ListRow>
  );
}

function EndpointGrid({
  targetAgentIds,
  selected,
  source,
  sourceAgentId,
  eligibilities,
  onSelect,
  onRetryEligibility,
  onOauthGuide,
}: {
  targetAgentIds: AgentId[];
  selected: AgentId | null;
  source: SourceOption['ref'];
  sourceAgentId: AgentId | null;
  eligibilities: ReadonlyMap<string, PlanEligibility>;
  onSelect: (agentId: AgentId) => void;
  onRetryEligibility: (request: { source: SourceOption['ref']; targetAgentId: AgentId }) => void;
  onOauthGuide: (agentId: AgentId) => void;
}) {
  const { t } = useI18n();
  return (
    <div className="grid grid-cols-1 gap-2">
      {ROUTE_ENDPOINTS.map((endpoint) => {
        const agents = agentsForRouteEndpoint(
          endpoint.id,
          targetAgentIds,
          source,
          eligibilities,
        );
        const representative = representativeAgentForRouteEndpoint(
          endpoint.id,
          targetAgentIds,
          source,
          eligibilities,
        );
        const eligibility = eligibilityForRouteEndpoint(
          endpoint.id,
          targetAgentIds,
          source,
          eligibilities,
        );
        const selectable = representative != null && isTargetSelectable(eligibility);
        const active = selected != null && agents.includes(selected);
        return (
          <div
            key={endpoint.id}
            role="button"
            tabIndex={selectable ? 0 : -1}
            aria-disabled={!selectable}
            onClick={() => {
              if (selectable && representative) onSelect(representative);
            }}
            onKeyDown={(event) => {
              if (!selectable || !representative) return;
              if (event.key === 'Enter' || event.key === ' ') {
                event.preventDefault();
                onSelect(representative);
              }
            }}
            className={cn(
              'rounded-card border border-border bg-panel p-3 text-left transition-colors',
              active && selectable && 'border-border-strong bg-active',
              !selectable && 'opacity-60',
              selectable && 'hover:bg-hover/50',
            )}
          >
            <div className="min-w-0">
              <RouteEndpointUrl
                path={endpoint.path}
                endpointId={endpoint.id}
                className="text-sm font-medium"
              />
              <p className="text-xs text-muted">
                {endpoint.id === 'messages'
                  ? t('connect.select.endpointMessages')
                  : endpoint.id === 'responses'
                    ? t('connect.select.endpointResponses')
                    : t('connect.select.endpointChat')}
              </p>
            </div>
            {representative ? (
              <EligibilityBody
                eligibility={eligibility}
                onRetry={() => onRetryEligibility({ source, targetAgentId: representative })}
                onOauthGuide={() => onOauthGuide(sourceAgentId ?? representative)}
              />
            ) : (
              <p className="mt-1 text-xs text-muted">{t('connect.select.endpointUnavailable')}</p>
            )}
          </div>
        );
      })}
    </div>
  );
}

function TargetGrid({
  targetAgentIds,
  selected,
  source,
  sourceAgentId,
  eligibilities,
  onSelect,
  onRetryEligibility,
  onOauthGuide,
}: {
  targetAgentIds: AgentId[];
  selected: AgentId | null;
  source: SourceOption['ref'];
  sourceAgentId: AgentId | null;
  eligibilities: ReadonlyMap<string, PlanEligibility>;
  onSelect: (agentId: AgentId) => void;
  onRetryEligibility: (request: { source: SourceOption['ref']; targetAgentId: AgentId }) => void;
  onOauthGuide: (agentId: AgentId) => void;
}) {
  const { t } = useI18n();
  if (targetAgentIds.length === 0) {
    return <p className="text-sm text-muted">{t('connect.select.noOtherAgents')}</p>;
  }
  return (
    <div className="grid grid-cols-2 gap-2">
      {targetAgentIds.map((agentId) => {
        const eligibility = eligibilityOf(eligibilities, source, agentId);
        const selectable = isTargetSelectable(eligibility);
        const active = selected === agentId;
        return (
          <div
            key={agentId}
            role="button"
            tabIndex={selectable ? 0 : -1}
            aria-disabled={!selectable}
            onClick={() => {
              if (selectable) onSelect(agentId);
            }}
            onKeyDown={(event) => {
              if (!selectable) return;
              if (event.key === 'Enter' || event.key === ' ') {
                event.preventDefault();
                onSelect(agentId);
              }
            }}
            className={cn(
              'rounded-card border border-border bg-panel p-3 text-left transition-colors',
              active && selectable && 'border-border-strong bg-active',
              !selectable && 'opacity-60',
              selectable && 'hover:bg-hover/50',
            )}
          >
            <div className="flex items-center gap-2">
              <AgentLogo agentId={agentId} size="sm" />
              <span className="text-sm font-medium">{agentDisplayName(agentId)}</span>
            </div>
            <EligibilityBody
              eligibility={eligibility}
              onRetry={() => onRetryEligibility({ source, targetAgentId: agentId })}
              onOauthGuide={() => onOauthGuide(sourceAgentId ?? agentId)}
            />
          </div>
        );
      })}
    </div>
  );
}

function EligibilityBody({
  eligibility,
  onRetry,
  onOauthGuide,
}: {
  eligibility: PlanEligibility | undefined;
  onRetry: () => void;
  onOauthGuide: () => void;
}) {
  const { t } = useI18n();
  if (!eligibility || eligibility.kind === 'loading') {
    return <Skeleton className="mt-2 h-3 w-28" />;
  }
  if (eligibility.kind === 'blocked_oauth') {
    return (
      <p className="mt-1 text-xs text-warning">
        {t('connect.select.oauthIncomplete')}{' '}
        <button
          type="button"
          className="underline"
          onClick={(event) => {
            event.stopPropagation();
            onOauthGuide();
          }}
        >
          {t('connect.select.goLogin')}
        </button>
      </p>
    );
  }
  if (eligibility.kind === 'error') {
    return (
      <p className="mt-1 flex items-center gap-2 text-xs text-danger">
        <span className="min-w-0 flex-1">{eligibility.message}</span>
        <Button
          size="sm"
          variant="outline"
          onClick={(event) => {
            event.stopPropagation();
            onRetry();
          }}
        >
          <RefreshCw className="h-3 w-3" /> {t('chrome.error.retry')}
        </Button>
      </p>
    );
  }
  const maturity = planMaturityLabel(eligibility.plan.maturity, t);
  const routeTitle = planRouteSummary(eligibility.plan, t);
  const routeLine = maturity
    ? `${routeTitle} · ${maturity}`
    : routeTitle;
  if (planEligibilityAllowsApply(eligibility)) {
    return <p className="mt-1 text-xs text-secondary">{routeLine}</p>;
  }
  return <p className="mt-1 text-xs text-warning">{eligibility.reason ?? routeLine}</p>;
}

function GuideActions({
  onGoImport,
  onGoNewKey,
}: {
  onGoImport: () => void;
  onGoNewKey: () => void;
}) {
  const { t } = useI18n();
  return (
    <section className="space-y-2 rounded-btn border border-border bg-subtle/60 p-3">
      <p className="text-xs font-medium text-secondary">{t('connect.select.otherWays')}</p>
      <div className="flex flex-wrap gap-2">
        <Button size="sm" variant="outline" onClick={onGoImport}>
          <Wallet className="h-3.5 w-3.5" />
          {t('connect.select.importLogin')}
        </Button>
        <Button size="sm" variant="outline" onClick={onGoNewKey}>
          <KeyRound className="h-3.5 w-3.5" />
          {t('connect.select.newApiKey')}
        </Button>
      </div>
      <p className="text-xs text-muted">
        {t('connect.select.otherWaysHint')}
      </p>
    </section>
  );
}
