/**
 * Global ticket wallet list UI (Connections).
 * Data from listTicketWallet; per-row 用到其他工具 / 本机转发 for true tickets.
 * 「详情」is a read-only expand; edit/delete stay secondary actions inside it.
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
import { AgentDot } from '@/components/shared/AgentDot';
import { DetailRow } from '@/components/shared/DetailRow';
import { EmptyState } from '@/components/shared/EmptyState';
import { ListRow } from '@/components/shared/ListRow';
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
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Hint, Tip } from '@/components/ui/tooltip';
import { useI18n } from '@/components/shared/LanguageProvider';
import { RouteEndpointUrl } from '@/components/shared/RouteEndpointUrl';
import { agentDisplayName, resolveAgentMeta } from '@/config/agents';
import type { TicketView, TicketWallet } from '@/lib/backend/contracts/ticket';
import type { AgentId } from '@/lib/types';
import { cn } from '@/lib/utils';
import {
  buildTicketAddMenu,
  focusedTicketAddAgent,
  handleTicketAddMenuSelect,
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
  ticketSurfaceChipLabel,
  type TicketAddMenuAgent,
  type TicketDetailExtras,
  type TicketDetailField,
  type TicketWalletRow,
} from './ticket-wallet-model';

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
    const label = t('connections.list.oauthAccount');
    return (
      <Hint label={label}>
        <span className="inline-flex" style={{ color }} aria-label={label}>
          <CircleUser className="h-4 w-4" strokeWidth={1.8} />
        </span>
      </Hint>
    );
  }
  if (cls === 'api_key') {
    const label = t('connections.list.apiKeyAuth');
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

const HIDDEN_ADVANCED_LABELS = new Set(['导入自', '登录状态', 'Imported from', 'Login status']);

export function TicketDetailPanel({
  id,
  advanced,
  extras,
  editLabel,
  refreshing,
  refreshLocked,
  onRefresh,
  onEdit,
  onDelete,
}: {
  id: string;
  advanced: TicketDetailField[];
  extras?: TicketDetailExtras | null;
  editLabel?: string | null;
  refreshing?: boolean;
  refreshLocked?: boolean;
  onRefresh?: () => void;
  onEdit?: () => void;
  onDelete: () => void;
}) {
  const { t } = useI18n();
  const has7d = hasOfficialQuotaWindow(extras?.quota7dPct);
  const has5h = hasOfficialQuotaWindow(extras?.quota5hPct);
  const hasQuota = has7d || has5h;
  const visibleAdvanced = advanced.filter((field) => !HIDDEN_ADVANCED_LABELS.has(field.label));
  const isSyncLogin = extras?.oauthAction?.kind === 'sync-current-login';
  const refreshLabel = isSyncLogin
    ? t('connections.list.syncCurrentLogin')
    : t('connections.list.refresh');
  const refreshBusyLabel = isSyncLogin
    ? t('connections.list.syncing')
    : t('connections.list.refreshing');

  return (
    <Card
      id={id}
      variant="plain"
      className="mt-3 flex flex-col gap-3 bg-canvas p-3 text-xs"
    >
      {hasQuota ? (
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
          </div>
        </div>
      ) : null}

      {extras?.refreshTokenPreview ? (
        <DetailRow
          label={t('connections.list.refreshToken')}
          value={extras.refreshTokenPreview}
          mono
        />
      ) : null}

      {visibleAdvanced.length > 0 ? (
        <details>
          <summary className="cursor-pointer text-meta text-muted">{t('connections.list.more')}</summary>
          <div className="mt-1.5 grid gap-1.5 text-secondary sm:grid-cols-2">
            {visibleAdvanced.map((field) => (
              <DetailRow
                key={`${field.label}:${field.value}`}
                label={field.label}
                value={field.value}
                mono={field.mono}
              />
            ))}
          </div>
        </details>
      ) : null}

      <div className="flex flex-wrap items-center justify-end gap-2 pt-1">
        <div className="flex flex-wrap items-center gap-2">
          {extras?.oauthAction && onRefresh ? (
            <Button
              size="sm"
              variant="secondary"
              disabled={refreshLocked || refreshing}
              aria-label={refreshLabel}
              onClick={onRefresh}
            >
              <RefreshCw className={cn('h-3.5 w-3.5', refreshing && 'animate-spin')} />
              {refreshing ? refreshBusyLabel : refreshLabel}
            </Button>
          ) : null}
          {editLabel && onEdit ? (
            <Button size="sm" variant="secondary" onClick={onEdit}>
              <Pencil className="h-3.5 w-3.5" /> {editLabel}
            </Button>
          ) : null}
          <Button
            size="sm"
            variant="dangerOutline"
            title={extras?.isCurrent ? t('connections.list.moveToTrashCurrentTip') : undefined}
            onClick={onDelete}
          >
            <Trash2 className="h-3.5 w-3.5" /> {t('connections.list.moveToTrash')}
          </Button>
        </div>
      </div>
    </Card>
  );
}

function TicketRow({
  row,
  extras,
  refreshingId,
  switchingId,
  nativeSwitch,
  onShare,
  onRoute,
  onSwitch,
  onRefresh,
  onEdit,
  onDelete,
}: {
  row: TicketWalletRow;
  extras: TicketDetailExtras | null;
  refreshingId: string | null;
  switchingId: string | null;
  nativeSwitch: boolean;
  onShare: (ticket: TicketView) => void;
  onRoute: (ticket: TicketView) => void;
  onSwitch?: (ticket: TicketView) => void;
  onRefresh?: (ticket: TicketView) => void;
  onEdit: (ticket: TicketView) => void;
  onDelete: (ticket: TicketView) => void;
}) {
  const { t } = useI18n();
  const { ticket, usageParts, highlighted } = row;
  const [expanded, setExpanded] = React.useState(false);
  const detailsId = React.useId();
  const editLabel = ticketDetailEditLabel(extras, t);
  const authChip = ticketAuthChip(extras);
  const switchChip = ticketSwitchChip(extras, t);
  const switching = switchingId === ticket.id;
  const switchBusy = switchingId !== null;
  const title = ticketCardTitle(ticket, extras);

  return (
    <ListRow
      active={highlighted}
      indicatorColor={resolveAgentMeta(ticket.agentId).color}
      className="p-3"
    >
      <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
        <div className="flex min-w-0 flex-1 flex-wrap items-center gap-x-2 gap-y-1">
          <AgentDot agentId={ticket.agentId} />
          <CredentialMark cls={ticket.credentialClass} agentId={ticket.agentId} />
          <Tip className="truncate text-body font-medium" label={title}>
            {title}
          </Tip>
          <Badge variant={ticket.surface === 'unknown' ? 'accent' : 'default'}>
            {ticketSurfaceChipLabel(ticket.surface, t)}
          </Badge>
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
        </div>
        <div className="flex shrink-0 items-center gap-2">
          {nativeSwitch ? (
            <Button
              size="sm"
              variant="outline"
              disabled={switchChip.kind === 'in-use' || switchBusy || !onSwitch}
              aria-label={switchChip.label}
              onClick={() => {
                if (switchChip.kind === 'in-use' || !onSwitch) return;
                onSwitch(ticket);
              }}
            >
              {switching ? t('connections.list.switching') : switchChip.label}
            </Button>
          ) : null}
          <Button size="sm" variant="outline" onClick={() => onShare(ticket)}>
            <Share2 className="h-3.5 w-3.5" /> {t('connections.list.share')}
          </Button>
          <Button size="sm" variant="outline" onClick={() => onRoute(ticket)}>
            <Cable className="h-3.5 w-3.5" /> {t('connections.list.route')}
          </Button>
          <Button
            size="sm"
            variant="ghost"
            aria-expanded={expanded}
            aria-controls={detailsId}
            onClick={() => setExpanded((open) => !open)}
          >
            {t('connections.list.details')}
            <ChevronDown
              className={cn('h-3.5 w-3.5 transition-transform', expanded && 'rotate-180')}
            />
          </Button>
        </div>
      </div>
      {expanded ? (
        <TicketDetailPanel
          id={detailsId}
          advanced={buildTicketDetailFields(ticket, extras, t).advanced}
          extras={extras}
          editLabel={editLabel}
          refreshing={refreshingId === ticket.id}
          refreshLocked={refreshingId !== null}
          onRefresh={extras?.oauthAction && onRefresh ? () => onRefresh(ticket) : undefined}
          onEdit={editLabel ? () => onEdit(ticket) : undefined}
          onDelete={() => onDelete(ticket)}
        />
      ) : null}
    </ListRow>
  );
}

export function TicketAddMenu({
  agents,
  focusedAgentId = null,
  onImportLogin,
  onAddKey,
}: {
  agents: TicketAddMenuAgent[];
  focusedAgentId?: AgentId | null;
  onImportLogin?: (agentId: AgentId) => void;
  onAddKey?: (agentId: AgentId) => void;
}) {
  const { t } = useI18n();
  const [open, setOpen] = React.useState(false);
  const focused = focusedTicketAddAgent(agents, focusedAgentId);

  const renderActions = (agent: TicketAddMenuAgent) =>
    agent.actions.map((action) => (
      <DropdownMenuItem
        key={action.kind}
        disabled={action.kind === 'import-login' ? !onImportLogin : !onAddKey}
        onSelect={(event) =>
          handleTicketAddMenuSelect(event, action.kind, agent.id, {
            onImportLogin,
            onAddKey,
            onMenuClose: () => setOpen(false),
          })
        }
      >
        {ticketAddActionLabel(action.kind, t)}
      </DropdownMenuItem>
    ));

  return (
    <DropdownMenu open={open} onOpenChange={setOpen}>
      <DropdownMenuTrigger asChild>
        <Button>
          <Plus className="h-4 w-4" /> {t('connections.list.add')} <ChevronDown className="h-3.5 w-3.5" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent
        align="end"
        className="min-w-[12rem]"
        onCloseAutoFocus={(event) => event.preventDefault()}
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
        ) : (
          <>
            <DropdownMenuLabel>{t('connections.list.addAgent')}</DropdownMenuLabel>
            {agents.map((agent) => (
              <DropdownMenuSub key={agent.id}>
                <DropdownMenuSubTrigger className="justify-between gap-2">
                  <span className="flex min-w-0 items-center gap-2">
                    <AgentDot agentId={agent.id} size="sm" title={null} />
                    <span className="truncate">{agent.name}</span>
                  </span>
                  <ChevronRight className="h-3.5 w-3.5 shrink-0 text-muted" />
                </DropdownMenuSubTrigger>
                <DropdownMenuSubContent className="min-w-[10rem]">
                  {renderActions(agent)}
                </DropdownMenuSubContent>
              </DropdownMenuSub>
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
  onSwitchTicket,
  onRefreshTicket,
  refreshingTicketId,
  switchingTicketId,
  extrasForTicket,
  onEditTicket,
  onDeleteTicket,
  onAddKey,
  onImportLogin,
  onClearAgentFilter,
  installedAgentIds,
}: {
  wallet: TicketWallet | null;
  loading?: boolean;
  highlightAgentId?: AgentId | null;
  agentFilterId?: AgentId | null;
  onShareTicket: (ticket: TicketView) => void;
  onRouteTicket: (ticket: TicketView) => void;
  onSwitchTicket?: (ticket: TicketView) => void;
  onRefreshTicket?: (ticket: TicketView) => void;
  refreshingTicketId?: string | null;
  switchingTicketId?: string | null;
  extrasForTicket?: (ticket: TicketView) => TicketDetailExtras | null;
  onEditTicket: (ticket: TicketView) => void;
  onDeleteTicket: (ticket: TicketView) => void;
  onAddKey?: (agentId: AgentId) => void;
  onImportLogin?: (agentId: AgentId) => void;
  onClearAgentFilter?: () => void;
  installedAgentIds?: readonly AgentId[];
}) {
  const { t } = useI18n();

  const tickets = wallet?.tickets ?? [];
  const rows = React.useMemo(() => {
    if (!wallet) return [];
    try {
      return buildTicketWalletRows(wallet, {
        highlightAgentId: highlightAgentId ?? null,
        agentFilterId,
        t,
      });
    } catch {
      return [];
    }
  }, [wallet, highlightAgentId, agentFilterId, t]);
  const addAgents = React.useMemo(
    () => buildTicketAddMenu(installedAgentIds),
    [installedAgentIds],
  );

  const renderAddMenu = () => (
    <TicketAddMenu
      agents={addAgents}
      focusedAgentId={agentFilterId}
      onImportLogin={onImportLogin}
      onAddKey={onAddKey}
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
          action={renderAddMenu()}
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
          {rows.map((row) => (
            <TicketRow
              key={row.ticket.id}
              row={row}
              extras={extrasForTicket?.(row.ticket) ?? null}
              refreshingId={refreshingTicketId ?? null}
              switchingId={switchingTicketId ?? null}
              nativeSwitch={showsNativeSwitch(row.ticket.agentId, agentFilterId)}
              onShare={onShareTicket}
              onRoute={onRouteTicket}
              onSwitch={onSwitchTicket}
              onRefresh={onRefreshTicket}
              onEdit={onEditTicket}
              onDelete={onDeleteTicket}
            />
          ))}
        </div>
      ) : null}

      {wallet ? (
        <p className="mt-3 text-meta text-muted">
          {t('connections.list.count', { n: tickets.length })}
          {highlightAgentId
            ? t('connections.list.highlighted', { name: agentDisplayName(highlightAgentId) })
            : ''}
        </p>
      ) : null}
    </div>
  );
}
