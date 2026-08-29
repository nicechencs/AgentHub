/**
 * Global ticket wallet list UI (Connections).
 * Data from listTicketWallet; per-row 用到其他工具 / 本机转发 for true tickets.
 * Click the card to open details in the workbench inspect pane; edit stays a button.
 */
import * as React from 'react';
import { Link } from 'react-router-dom';
import {
  Cable,
  ChevronDown,
  ChevronRight,
  CircleUser,
  KeyRound,
  Pencil,
  Plus,
  RefreshCw,
  Share2,
  Trash2,
} from 'lucide-react';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { SideInspectPanel } from '@/components/layout/SideInspectPanel';
import { AgentDot } from '@/components/shared/AgentDot';
import { DetailRow } from '@/components/shared/DetailRow';
import { EmptyState } from '@/components/shared/EmptyState';
import { AgentLogo } from '@/components/shared/AgentLogo';
import { LIST_ROW_PAD, ListRow, ListRowBody } from '@/components/shared/ListRow';
import { SortHandle } from '@/components/shared/SortHandle';
import { useSortableDrag } from '@/components/shared/use-sortable-drag';
import { useStoredIdOrder } from '@/components/shared/use-stored-id-order';
import { applyIdOrder } from '@/lib/list-order';
import { StorageKey } from '@/lib/ui-preferences';
import { QuotaBar } from '@/components/shared/QuotaBar';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import { ListSkeleton } from '@/components/ui/skeleton';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Hint, Tip } from '@/components/ui/tooltip';
import { useI18n } from '@/components/shared/LanguageProvider';
import { RouteEndpointUrl } from '@/components/shared/RouteEndpointUrl';
import { agentDisplayName, resolveAgentMeta } from '@/config/agents';
import type { BindingView, TicketView, TicketWallet } from '@/lib/backend/contracts/ticket';
import type { AgentId } from '@/lib/types';
import { cn } from '@/lib/utils';
import {
  buildTicketAddMenu,
  focusedTicketAddAgent,
  handleTicketAddMenuSelect,
  ticketAddMenuClosesOnKey,
  buildTicketDetailFields,
  buildTicketWalletRows,
  hasOfficialQuotaWindow,
  ticketAddActionLabel,
  ticketAuthChip,
  ticketCardTitle,
  showsNativeSwitch,
  ticketSwitchChip,
  ticketCredentialClassChipLabel,
  ticketDetailEditLabel,
  ticketRefreshDisabledReason,
  ticketSwitchDisabledReason,
  type TicketAddMenuAgent,
  type TicketBindAction,
  type TicketBindingRowView,
  type TicketDetailExtras,
  type TicketDetailField,
  type TicketWalletRow,
} from './ticket-wallet-model';
import { TicketAuthFiles } from './ticket-auth-files';
import { useOAuthLoginAgents } from './use-oauth-login-agents';

function CredentialMark({
  cls,
  agentId,
}: {
  cls: TicketView['credentialClass'];
  agentId: AgentId;
}) {
  const { t } = useI18n();
  const color = resolveAgentMeta(agentId).color;
  if (cls === 'oauth') {
    const label = t('kind.oauth');
    return (
      <Hint label={label}>
        <span className="inline-flex" style={{ color }} aria-label={label}>
          <CircleUser className="h-4 w-4" strokeWidth={1.8} />
        </span>
      </Hint>
    );
  }
  if (cls === 'api_key') {
    const label = t('kind.apikey');
    return (
      <Hint label={label}>
        <span className="inline-flex" style={{ color }} aria-label={label}>
          <KeyRound className="h-4 w-4" strokeWidth={1.8} />
        </span>
      </Hint>
    );
  }
  return (
    <Badge variant="accent">{ticketCredentialClassChipLabel(cls, t)}</Badge>
  );
}

function DisabledReasonButton({
  disabled,
  reason,
  ariaLabel,
  onClick,
  children,
  variant = 'outline',
}: {
  disabled: boolean;
  reason?: string;
  ariaLabel: string;
  onClick: () => void;
  children: React.ReactNode;
  variant?: 'outline' | 'secondary' | 'dangerOutline';
}) {
  return (
    <Hint key={disabled ? `${ariaLabel}:${reason ?? 'disabled'}` : `${ariaLabel}:enabled`} label={disabled ? (reason || ariaLabel) : undefined}>
      <Button
        size="sm"
        variant={variant}
        disabled={disabled}
        aria-label={ariaLabel}
        onClick={() => {
          if (disabled) return;
          onClick();
        }}
      >
        {children}
        {disabled && reason ? <span className="sr-only">{reason}</span> : null}
      </Button>
    </Hint>
  );
}

const HIDDEN_ADVANCED_LABELS = new Set(['导入自', '登录状态', 'Imported from', 'Login status']);

export function TicketDetailPanel({
  id,
  advanced = [],
  extras,
  ticket,
  bindings,
  refreshing,
  refreshLocked,
  onRefresh,
  onDelete,
  onEdit,
  asPanel = false,
  open = true,
  onOpenChange,
  width,
}: {
  id: string;
  advanced?: TicketDetailField[];
  extras?: TicketDetailExtras | null;
  ticket?: TicketView;
  bindings?: readonly BindingView[];
  refreshing?: boolean;
  refreshLocked?: boolean;
  onRefresh?: () => void;
  onDelete: () => void;
  onEdit?: () => void;
  asPanel?: boolean;
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
  width?: number;
}) {
  const { t } = useI18n();
  const computed = ticket
    ? buildTicketDetailFields(ticket, extras, t, bindings)
    : null;
  const advancedFields = computed?.advanced ?? advanced;
  const protocol = computed?.protocol ?? null;
  const bindingRows = computed?.bindingRows ?? [];
  const diagnostics = computed?.diagnostics ?? [];
  const timeline = computed?.timeline ?? [];
  const tokenRemaining = computed?.tokenRemaining ?? null;
  const localRouteLabel = t('kind.route.localRoute');
  const protocolLabelText = t('connections.list.protocol');
  const visibleAdvanced = advancedFields.filter((field) => {
    if (HIDDEN_ADVANCED_LABELS.has(field.label)) return false;
    if (bindingRows.length > 0 && field.label === localRouteLabel) return false;
    return true;
  });
  const overview: TicketDetailField[] = [];
  if (protocol && !visibleAdvanced.some((field) => field.label === protocolLabelText)) {
    overview.push({ label: protocolLabelText, value: protocol });
  }
  overview.push(...visibleAdvanced);
  // 5h is official-only. Missing quota5hPct hides the bar; never copy 7d into 5h.
  const has7d = hasOfficialQuotaWindow(extras?.quota7dPct);
  const has5h = hasOfficialQuotaWindow(extras?.quota5hPct);
  const hasQuota = has7d || has5h;
  const isSyncLogin = extras?.oauthAction?.kind === 'sync-current-login';
  const refreshLabel = isSyncLogin
    ? t('connections.list.syncCurrentLogin')
    : t('connections.list.refresh');
  const refreshBusyLabel = isSyncLogin
    ? t('connections.list.syncing')
    : t('connections.list.refreshing');
  const editLabel = ticketDetailEditLabel(extras, t);
  const title = ticket
    ? ticketCardTitle(ticket, extras)
    : t('connections.detailTitle');

  const requestDelete = () => {
    if (asPanel) onOpenChange?.(false);
    onDelete();
  };
  const refreshButton = extras?.oauthAction && onRefresh ? (
    <DisabledReasonButton
      variant="secondary"
      disabled={Boolean(refreshLocked || refreshing)}
      reason={ticketRefreshDisabledReason({
        refreshing: Boolean(refreshing),
        refreshLocked: Boolean(refreshLocked),
        busyLabel: refreshBusyLabel,
      }, t)}
      ariaLabel={refreshLabel}
      onClick={onRefresh}
    >
      <RefreshCw className={cn('h-3.5 w-3.5', refreshing && 'animate-spin')} />
      {refreshing ? refreshBusyLabel : refreshLabel}
    </DisabledReasonButton>
  ) : null;
  const deleteButton = (
    <Hint label={extras?.isCurrent ? t('connections.list.moveToTrashCurrentTip') : undefined}>
      <Button
        size="sm"
        variant="dangerOutline"
        onClick={requestDelete}
      >
        <Trash2 className="h-3.5 w-3.5" /> {t('connections.list.moveToTrash')}
      </Button>
    </Hint>
  );
  const actions = (
    <div className="flex flex-wrap items-center gap-2">
      {refreshButton}
      {deleteButton}
    </div>
  );
  const body = (
    <TicketDetailBody
      extras={extras}
      hasQuota={hasQuota}
      has7d={has7d}
      has5h={has5h}
      overview={overview}
      timeline={timeline}
      tokenRemaining={tokenRemaining}
      bindingRows={ticket ? bindingRows : []}
      diagnostics={ticket ? diagnostics : []}
      showClients={Boolean(ticket)}
      agentId={ticket?.agentId}
      files={extras?.credentialFiles}
    />
  );

  if (asPanel) {
    if (!open) return null;
    return (
      <SideInspectPanel
        title={t('connections.detailTitle')}
        description={title === t('connections.detailTitle') ? t('connections.detailDescription') : title}
        onClose={() => onOpenChange?.(false)}
        width={width}
        headerActions={(
          <>
            {refreshButton}
            {deleteButton}
            {onEdit && editLabel ? (
              <Button type="button" size="sm" variant="outline" onClick={onEdit}>
                {editLabel}
              </Button>
            ) : null}
          </>
        )}
      >
        <div id={id} data-ticket-detail={ticket?.id ?? id}>
          {body}
        </div>
      </SideInspectPanel>
    );
  }

  return (
    <Card
      id={id}
      variant="plain"
      className="mt-3 flex flex-col gap-3 bg-canvas p-3 text-xs"
    >
      {body}
      <div className="flex flex-wrap items-center justify-end gap-2 pt-1">
        {actions}
      </div>
    </Card>
  );
}

function TicketDetailBody({
  extras,
  hasQuota,
  has7d,
  has5h,
  overview,
  timeline,
  tokenRemaining,
  bindingRows,
  diagnostics,
  showClients,
  agentId,
  files,
}: {
  extras?: TicketDetailExtras | null;
  hasQuota: boolean;
  has7d: boolean;
  has5h: boolean;
  overview: TicketDetailField[];
  timeline: TicketDetailField[];
  tokenRemaining: string | null;
  bindingRows: TicketBindingRowView[];
  diagnostics: TicketDetailField[];
  showClients: boolean;
  agentId?: AgentId;
  files?: TicketDetailExtras['credentialFiles'];
}) {
  const { t } = useI18n();
  return (
    <div className="flex flex-col gap-3 text-xs">
      {hasQuota || tokenRemaining ? (
        <div>
          <p className="text-meta text-muted">{t('connections.list.usage')}</p>
          <div className="mt-1.5 flex flex-col gap-1.5">
            {has7d ? (
              <QuotaBar
                label="7d"
                pct={extras?.quota7dPct}
                resetIn={extras?.quota7dResetIn}
              />
            ) : null}
            {has5h ? (
              <QuotaBar
                label="5h"
                pct={extras?.quota5hPct}
                resetIn={extras?.quotaResetIn}
              />
            ) : null}
            {tokenRemaining ? (
              <DetailRow
                label={t('connections.list.tokenRemaining')}
                value={tokenRemaining}
              />
            ) : null}
          </div>
        </div>
      ) : null}

      {overview.length > 0 ? (
        <div className="grid gap-1.5 text-secondary sm:grid-cols-2">
          {overview.map((field) => (
            <DetailRow
              key={`${field.label}:${field.value}`}
              label={field.label}
              value={field.value}
              mono={field.mono}
              copyable={field.copyable}
            />
          ))}
        </div>
      ) : null}

      {showClients ? (
        <section className="space-y-1.5">
          <h3 className="text-sm font-medium">{t('connections.list.clientsTitle')}</h3>
          {bindingRows.length === 0 ? (
            <p className="text-sm text-muted">{t('connections.list.clientsEmpty')}</p>
          ) : (
            <ul className="space-y-1">
              {bindingRows.map((row) => (
                <li
                  key={`${row.agentId}:${row.routeLabel}:${row.localUrl ?? ''}`}
                  className="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1 py-1"
                >
                  <span className="flex min-w-[5.5rem] items-center gap-1.5 text-sm font-medium">
                    <AgentDot agentId={row.agentId} size="sm" title={null} />
                    <span className="truncate">{row.agentLabel}</span>
                  </span>
                  <span className="shrink-0 text-meta text-muted">{row.routeLabel}</span>
                  <span className="shrink-0 text-meta text-secondary">{row.status}</span>
                  {row.localUrl ? (
                    <span className="min-w-0 flex-1 truncate font-mono text-xs text-secondary">{row.localUrl}</span>
                  ) : null}
                </li>
              ))}
            </ul>
          )}
        </section>
      ) : null}

      {timeline.length > 0 ? (
        <section className="space-y-1.5">
          <h3 className="text-sm font-medium">{t('connections.list.timelineTitle')}</h3>
          <div className="grid gap-1.5 text-secondary sm:grid-cols-2">
            {timeline.map((field) => (
              <DetailRow
                key={`${field.label}:${field.value}`}
                label={field.label}
                value={field.value}
                mono={field.mono}
              />
            ))}
          </div>
        </section>
      ) : null}

      {extras?.refreshTokenPreview ? (
        <DetailRow
          label={t('connections.list.refreshToken')}
          value={extras.refreshTokenPreview}
          mono
        />
      ) : null}

      {diagnostics.length > 0 ? (
        <details className="group rounded-card border border-border bg-subtle/60">
          <summary className="flex cursor-pointer list-none items-center justify-between gap-2 px-3 py-2 text-xs font-medium text-secondary marker:content-none [&::-webkit-details-marker]:hidden">
            <span>{t('connections.list.diagnostics')}</span>
            <ChevronDown className="h-3.5 w-3.5 shrink-0 transition-transform group-open:rotate-180" aria-hidden />
          </summary>
          <div className="grid gap-1.5 border-t border-border px-3 py-3 text-xs">
            {diagnostics.map((field) => (
              <DetailRow
                key={`${field.label}:${field.value}`}
                label={field.label}
                value={field.value}
                mono={field.mono}
                copyable={field.copyable}
              />
            ))}
          </div>
        </details>
      ) : null}

      {agentId && files && files.length > 0 ? (
        <TicketAuthFiles agentId={agentId} files={files} />
      ) : null}
    </div>
  );
}

function TicketRow({
  row,
  extras,
  switchingId,
  nativeSwitch,
  onShare,
  onRoute,
  shareAction,
  routeAction,
  onSwitch,
  onEdit,
  onShowDetail,
  active,
  suppressHighlight,
  sortHandle,
}: {
  row: TicketWalletRow;
  extras: TicketDetailExtras | null;
  switchingId: string | null;
  nativeSwitch: boolean;
  onShare: (ticket: TicketView) => void;
  onRoute: (ticket: TicketView) => void;
  shareAction: TicketBindAction;
  routeAction: TicketBindAction;
  onSwitch?: (ticket: TicketView) => void;
  onEdit: (ticket: TicketView) => void;
  onShowDetail?: (ticket: TicketView) => void;
  active: boolean;
  suppressHighlight?: boolean;
  sortHandle?: React.ReactNode;
}) {
  const { t } = useI18n();
  const { ticket, usageParts, highlighted } = row;
  const editLabel = ticketDetailEditLabel(extras, t);
  const authChip = ticketAuthChip(extras);
  const occupancy = resolveAgentMeta(ticket.agentId).occupancy;
  const switchChip = ticketSwitchChip(extras, t, {
    occupancy,
    agentName: agentDisplayName(ticket.agentId),
  });
  const switching = switchingId === ticket.id;
  const switchBusy = switchingId !== null;
  const title = ticketCardTitle(ticket, extras);

  return (
    <ListRow
      active={active || (highlighted && !suppressHighlight)}
      indicatorColor={resolveAgentMeta(ticket.agentId).color}
      className={LIST_ROW_PAD}
      onOpen={onShowDetail ? () => onShowDetail(ticket) : undefined}
    >
      <ListRowBody
        leading={sortHandle}
        main={(
          <>
            <AgentLogo agentId={ticket.agentId} size="sm" />
            <CredentialMark cls={ticket.credentialClass} agentId={ticket.agentId} />
            <Tip className="truncate text-body font-medium" label={title}>
              {title}
            </Tip>
            {authChip ? (
              <Badge variant="default" className={authChip.mono ? 'font-mono' : undefined}>
                {authChip.label}
              </Badge>
            ) : null}
            <span className="text-meta text-secondary">
              {(usageParts ?? []).map((part, index) => (
                part.kind === 'bridge' ? (
                  <Link
                    key={`${part.href}:${index}`}
                    to={part.href}
                    className="text-info underline"
                    onClick={(event) => event.stopPropagation()}
                  >
                    {part.label}
                  </Link>
                ) : part.kind === 'endpoint' ? (
                  <RouteEndpointUrl
                    key={`endpoint:${index}`}
                    path={part.path}
                    port={part.port}
                    endpointId={part.endpointId}
                    className="text-meta"
                  />
                ) : (
                  <span key={`text:${index}`}>{part.text}</span>
                )
              ))}
            </span>
          </>
        )}
        actions={(
          <>
            {nativeSwitch ? (
              <DisabledReasonButton
                disabled={switchChip.kind === 'in-use' || switchBusy || !onSwitch}
                reason={ticketSwitchDisabledReason({
                  kind: switchChip.kind,
                  switchBusy,
                  canSwitch: Boolean(onSwitch),
                  occupancy,
                }, t)}
                ariaLabel={switchChip.label}
                onClick={() => {
                  if (!onSwitch) return;
                  onSwitch(ticket);
                }}
              >
                {switching ? t('connections.list.switching') : switchChip.label}
              </DisabledReasonButton>
            ) : null}
            <DisabledReasonButton
              disabled={shareAction.disabled}
              reason={shareAction.disabled ? shareAction.reason : undefined}
              ariaLabel={t('connections.list.share')}
              onClick={() => onShare(ticket)}
            >
              <Share2 className="h-3.5 w-3.5" /> {t('connections.list.share')}
            </DisabledReasonButton>
            <DisabledReasonButton
              disabled={routeAction.disabled}
              reason={routeAction.disabled ? routeAction.reason : undefined}
              ariaLabel={t('connections.list.route')}
              onClick={() => onRoute(ticket)}
            >
              <Cable className="h-3.5 w-3.5" /> {t('connections.list.route')}
            </DisabledReasonButton>
            {editLabel ? (
              <Button size="sm" variant="outline" onClick={() => onEdit(ticket)}>
                <Pencil className="h-3.5 w-3.5" /> {editLabel}
              </Button>
            ) : null}
          </>
        )}
      />
    </ListRow>
  );
}

export function TicketAddMenu({
  agents,
  focusedAgentId = null,
  onImportLogin,
  onOauth,
  onAddKey,
  variant = 'default',
}: {
  agents: TicketAddMenuAgent[];
  focusedAgentId?: AgentId | null;
  onImportLogin?: (agentId: AgentId) => void;
  onOauth?: (agentId: AgentId) => void;
  onAddKey?: (agentId: AgentId) => void;
  variant?: 'default' | 'outline' | 'secondary';
}) {
  const { t } = useI18n();
  const [open, setOpen] = React.useState(false);
  const [expandedId, setExpandedId] = React.useState<AgentId | null>(null);
  const focused = focusedTicketAddAgent(agents, focusedAgentId);
  const expanded = expandedId ? agents.find((agent) => agent.id === expandedId) ?? null : null;

  React.useEffect(() => {
    if (!open) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (!ticketAddMenuClosesOnKey(event.key)) return;
      event.preventDefault();
      setOpen(false);
      setExpandedId(null);
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [open]);

  const renderActions = (agent: TicketAddMenuAgent) =>
    agent.actions.map((action) => (
      <DropdownMenuItem
        key={action.kind}
        disabled={
          action.kind === 'import-login'
            ? !onImportLogin
            : action.kind === 'oauth'
              ? !onOauth
              : !onAddKey
        }
        onSelect={(event) =>
          handleTicketAddMenuSelect(event, action.kind, agent.id, {
            onImportLogin,
            onOauth,
            onAddKey,
            onMenuClose: () => setOpen(false),
          })
        }
      >
        {ticketAddActionLabel(action.kind, t)}
      </DropdownMenuItem>
    ));

  return (
    <DropdownMenu
      modal={false}
      open={open}
      onOpenChange={(next) => {
        setOpen(next);
        if (!next) setExpandedId(null);
      }}
    >
      <DropdownMenuTrigger asChild>
        <Button variant={variant}>
          <Plus className="h-3.5 w-3.5" /> {t('connections.list.add')} <ChevronDown className="h-3.5 w-3.5" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent
        align="end"
        className="min-w-[12rem]"
        onCloseAutoFocus={(event) => event.preventDefault()}
        onEscapeKeyDown={(event) => {
          event.preventDefault();
          setOpen(false);
          setExpandedId(null);
        }}
      >
        {agents.length === 0 ? (
          <>
            <DropdownMenuLabel>{t('connections.list.addAgent')}</DropdownMenuLabel>
            <DropdownMenuItem disabled>{t('connections.list.noAddAgent')}</DropdownMenuItem>
          </>
        ) : focused ? (
          <>
            <DropdownMenuLabel>
              <span className="flex min-w-0 items-center gap-2">
                <AgentDot agentId={focused.id} size="sm" title={null} />
                <span className="truncate">{focused.name}</span>
              </span>
            </DropdownMenuLabel>
            {renderActions(focused)}
          </>
        ) : expanded ? (
          <>
            <DropdownMenuItem
              className="justify-between gap-2"
              onSelect={(event) => {
                event.preventDefault();
                setExpandedId(null);
              }}
            >
              <span className="flex min-w-0 items-center gap-2">
                <AgentDot agentId={expanded.id} size="sm" title={null} />
                <span className="truncate">{expanded.name}</span>
              </span>
            </DropdownMenuItem>
            {renderActions(expanded)}
          </>
        ) : (
          <>
            <DropdownMenuLabel>{t('connections.list.addAgent')}</DropdownMenuLabel>
            {agents.map((agent) => (
              <DropdownMenuItem
                key={agent.id}
                className="justify-between gap-2"
                onSelect={(event) => {
                  event.preventDefault();
                  setExpandedId(agent.id);
                }}
              >
                <span className="flex min-w-0 items-center gap-2">
                  <AgentDot agentId={agent.id} size="sm" title={null} />
                  <span className="truncate">{agent.name}</span>
                </span>
                <ChevronRight className="h-3.5 w-3.5 shrink-0 text-muted" />
              </DropdownMenuItem>
            ))}
          </>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

export function TicketWalletList({
  wallet,
  loading,
  highlightAgentId,
  agentFilterId = null,
  onShareTicket,
  onRouteTicket,
  shareActionForTicket,
  routeActionForTicket,
  onSwitchTicket,
  switchingTicketId,
  extrasForTicket,
  onEditTicket,
  onShowDetail,
  activeTicketId,
  onAddKey,
  onImportLogin,
  onOauth,
  onClearAgentFilter,
  installedAgentIds,
  oauthLoginAgents: oauthLoginAgentsProp,
}: {
  wallet: TicketWallet | null;
  loading?: boolean;
  highlightAgentId?: AgentId | null;
  agentFilterId?: AgentId | null;
  onShareTicket: (ticket: TicketView) => void;
  onRouteTicket: (ticket: TicketView) => void;
  shareActionForTicket?: (ticket: TicketView) => TicketBindAction;
  routeActionForTicket?: (ticket: TicketView) => TicketBindAction;
  onSwitchTicket?: (ticket: TicketView) => void;
  switchingTicketId?: string | null;
  extrasForTicket?: (ticket: TicketView) => TicketDetailExtras | null;
  onEditTicket: (ticket: TicketView) => void;
  onDeleteTicket: (ticket: TicketView) => void;
  onShowDetail?: (ticket: TicketView) => void;
  activeTicketId?: string | null;
  onAddKey?: (agentId: AgentId) => void;
  onImportLogin?: (agentId: AgentId) => void;
  onOauth?: (agentId: AgentId) => void;
  onClearAgentFilter?: () => void;
  installedAgentIds?: readonly AgentId[];
  oauthLoginAgents?: readonly AgentId[] | null;
}) {
  const { t } = useI18n();

  const tickets = wallet?.tickets ?? [];
  const { stored: ticketOrder, moveInLive, seedIfEmpty } = useStoredIdOrder(StorageKey.connectionsTicketOrder);
  const rows = React.useMemo(() => {
    if (!wallet) return [];
    try {
      const built = buildTicketWalletRows(wallet, {
        highlightAgentId: highlightAgentId ?? null,
        agentFilterId,
        t,
      });
      return applyIdOrder(built, (row) => row.ticket.id, ticketOrder);
    } catch {
      return [];
    }
  }, [wallet, highlightAgentId, agentFilterId, t, ticketOrder]);
  const liveIds = React.useMemo(() => rows.map((row) => row.ticket.id), [rows]);
  React.useEffect(() => {
    seedIfEmpty(liveIds);
  }, [liveIds, seedIfEmpty]);
  const canReorder = liveIds.length > 1;
  const { onDragStartId, rowProps } = useSortableDrag((fromId, toId) => {
    moveInLive(liveIds, fromId, toId);
  });
  const moveNeighbor = React.useCallback((id: string, direction: -1 | 1) => {
    const index = liveIds.indexOf(id);
    const next = liveIds[index + direction];
    if (!next) return;
    moveInLive(liveIds, id, next);
  }, [liveIds, moveInLive]);
  const fetchedOauthLoginAgents = useOAuthLoginAgents(
    oauthLoginAgentsProp === undefined ? installedAgentIds : null,
  );
  const oauthLoginAgents = oauthLoginAgentsProp ?? fetchedOauthLoginAgents;
  const addAgents = React.useMemo(
    () => buildTicketAddMenu(installedAgentIds, oauthLoginAgents),
    [installedAgentIds, oauthLoginAgents],
  );

  const renderAddMenu = (variant?: 'default' | 'outline' | 'secondary') => (
    <TicketAddMenu
      agents={addAgents}
      focusedAgentId={agentFilterId}
      onImportLogin={onImportLogin}
      onOauth={onOauth}
      onAddKey={onAddKey}
      variant={variant}
    />
  );

  return (
    <div>
      {loading && !wallet ? <ListSkeleton rows={4} /> : null}

      {wallet && tickets.length === 0 ? (
        <EmptyState
          icon={KeyRound}
          title={t('connections.list.emptyTitle')}
          description={t('connections.list.emptyDesc')}
          action={renderAddMenu('outline')}
        />
      ) : null}

      {wallet && tickets.length > 0 && rows.length === 0 ? (
        <EmptyState
          icon={KeyRound}
          title={t('connections.list.noMatchTitle')}
          description={t('connections.list.noMatchDesc')}
          action={
            <Button
              size="sm"
              variant="outline"
              className="mt-2"
              onClick={() => {
                onClearAgentFilter?.();
              }}
            >
              {t('connections.list.showAll')}
            </Button>
          }
        />
      ) : null}

      {rows.length > 0 ? (
        <div className={pageRhythm.stackDense}>
          {rows.map((row) => {
            const sortable = rowProps(row.ticket.id);
            return (
              <div key={row.ticket.id} {...sortable}>
                <TicketRow
                  row={row}
                  extras={extrasForTicket?.(row.ticket) ?? null}
                  switchingId={switchingTicketId ?? null}
                  nativeSwitch={showsNativeSwitch(row.ticket.agentId, agentFilterId)}
                  onShare={onShareTicket}
                  onRoute={onRouteTicket}
                  shareAction={shareActionForTicket?.(row.ticket) ?? { disabled: false }}
                  routeAction={routeActionForTicket?.(row.ticket) ?? { disabled: false }}
                  onSwitch={onSwitchTicket}
                  onEdit={onEditTicket}
                  onShowDetail={onShowDetail}
                  active={activeTicketId === row.ticket.id}
                  suppressHighlight={activeTicketId != null}
                  sortHandle={canReorder ? (
                    <SortHandle
                      id={row.ticket.id}
                      onDragStartId={onDragStartId}
                      onMoveNeighbor={moveNeighbor}
                    />
                  ) : null}
                />
              </div>
            );
          })}
        </div>
      ) : null}

      {wallet ? (
        <p className="mt-3 text-meta text-muted">
          {t('connections.list.count', { n: rows.length })}
          {highlightAgentId
            ? t('connections.list.highlighted', { name: agentDisplayName(highlightAgentId) })
            : ''}
        </p>
      ) : null}
    </div>
  );
}
