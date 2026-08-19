/**
 * Global ticket wallet list UI (Connections).
 * Data from listTicketWallet; per-row「接到…」always for true tickets.
 * 「详情」is a read-only expand; edit/delete stay secondary actions inside it.
 */
import * as React from 'react';
import { Link } from 'react-router-dom';
import { ChevronDown, ChevronRight, KeyRound, Pencil, Plus, Share2, Trash2 } from 'lucide-react';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { AgentDot } from '@/components/shared/AgentDot';
import { DetailRow } from '@/components/shared/DetailRow';
import { EmptyState } from '@/components/shared/EmptyState';
import { ListRow } from '@/components/shared/ListRow';
import { QuotaBar } from '@/components/shared/QuotaBar';
import { SearchField } from '@/components/shared/SearchField';
import { SegmentedControl } from '@/components/shared/SegmentedControl';
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
import { Tip } from '@/components/ui/tooltip';
import { useI18n } from '@/components/shared/LanguageProvider';
import { agentDisplayName, resolveAgentMeta } from '@/config/agents';
import type { TicketView, TicketWallet } from '@/lib/backend/contracts/ticket';
import type { AgentId } from '@/lib/types';
import { cn } from '@/lib/utils';
import {
  buildTicketAddMenu,
  handleTicketAddMenuSelect,
  buildTicketDetailFields,
  buildTicketWalletRows,
  countTicketsByFilter,
  formatTicketBindingDetailLines,
  humanizeTicketAuthLabel,
  ticketAddActionLabel,
  ticketCredentialClassChipLabel,
  ticketDetailEditLabel,
  ticketSurfaceChipLabel,
  ticketWalletFilterLabel,
  TICKET_WALLET_FILTERS,
  type TicketAddMenuAgent,
  type TicketBindingDetailLine,
  type TicketDetailExtras,
  type TicketDetailField,
  type TicketWalletFilter,
  type TicketWalletRow,
} from './ticket-wallet-model';

function credentialBadgeVariant(
  cls: TicketView['credentialClass'],
): 'default' | 'info' | 'accent' {
  if (cls === 'oauth') return 'default';
  if (cls === 'api_key') return 'info';
  return 'accent';
}

const HIDDEN_ADVANCED_LABELS = new Set(['导入自', '登录状态', 'Imported from', 'Login status']);

export function TicketDetailPanel({
  id,
  advanced,
  bindings,
  extras,
  importedFromLabel,
  editLabel,
  onEdit,
  onDelete,
}: {
  id: string;
  advanced: TicketDetailField[];
  bindings: TicketBindingDetailLine[];
  extras?: TicketDetailExtras | null;
  importedFromLabel?: string | null;
  editLabel?: string | null;
  onEdit?: () => void;
  onDelete: () => void;
}) {
  const { t } = useI18n();
  const has7d = extras?.quota7dPct != null;
  const has5h = extras?.quota5hPct != null;
  const hasQuota = has7d || has5h;
  const visibleAdvanced = advanced.filter((field) => !HIDDEN_ADVANCED_LABELS.has(field.label));

  return (
    <Card
      id={id}
      variant="plain"
      className="mt-3 flex flex-col gap-3 bg-canvas p-3 text-xs"
    >
      <div className={cn('grid gap-3', hasQuota && 'sm:grid-cols-2')}>
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

        <div>
          <p className="text-meta text-muted">{t('connections.list.usedOn')}</p>
          {bindings.length === 0 ? (
            <p className="mt-1.5 text-body text-secondary">{t('connections.list.unusedTools')}</p>
          ) : (
            <ul className="mt-1.5 space-y-1">
              {bindings.map((line) => (
                <li
                  key={`${line.agent}:${line.status}`}
                  className="flex items-baseline justify-between gap-3"
                >
                  <span className="text-body text-secondary">{line.agent}</span>
                  <span className="shrink-0 text-meta text-muted">{line.status}</span>
                </li>
              ))}
            </ul>
          )}
        </div>
      </div>

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

      <div className="flex flex-wrap items-center justify-between gap-2 pt-1">
        {importedFromLabel ? (
          <p className="text-meta text-muted">{importedFromLabel}</p>
        ) : (
          <span />
        )}
        <div className="ml-auto flex flex-wrap items-center gap-2">
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
  onConnect,
  onEdit,
  onDelete,
}: {
  row: TicketWalletRow;
  extras: TicketDetailExtras | null;
  onConnect: (ticket: TicketView) => void;
  onEdit: (ticket: TicketView) => void;
  onDelete: (ticket: TicketView) => void;
}) {
  const { t } = useI18n();
  const { ticket, usageParts, highlighted } = row;
  const [expanded, setExpanded] = React.useState(false);
  const detailsId = React.useId();
  const editLabel = ticketDetailEditLabel(extras, t);

  return (
    <ListRow
      active={highlighted}
      indicatorColor={resolveAgentMeta(ticket.agentId).color}
      className="p-3"
    >
      <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
        <div className="flex min-w-0 flex-1 flex-wrap items-center gap-x-2 gap-y-1">
          <AgentDot agentId={ticket.agentId} />
          <Tip className="truncate text-body font-medium" label={ticket.label}>
            {ticket.label}
          </Tip>
          <Badge variant={credentialBadgeVariant(ticket.credentialClass)}>
            {ticketCredentialClassChipLabel(ticket.credentialClass, t)}
          </Badge>
          <Badge variant={ticket.surface === 'unknown' ? 'accent' : 'default'}>
            {ticketSurfaceChipLabel(ticket.surface, t)}
          </Badge>
          {extras?.authLabel ? (
            <Badge variant="default">{humanizeTicketAuthLabel(extras.authLabel)}</Badge>
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
              ) : (
                <span key={`text:${index}`}>{part.text}</span>
              )
            ))}
          </span>
        </div>
        <div className="flex shrink-0 items-center gap-2">
          <Button size="sm" variant="outline" onClick={() => onConnect(ticket)}>
            <Share2 className="h-3.5 w-3.5" /> {t('connections.list.connect')}
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
          bindings={formatTicketBindingDetailLines(row.bindings, t)}
          extras={extras}
          importedFromLabel={
            ticket.importedFrom
              ? t('connections.list.importedFrom', { name: agentDisplayName(ticket.importedFrom) })
              : null
          }
          editLabel={editLabel}
          onEdit={editLabel ? () => onEdit(ticket) : undefined}
          onDelete={() => onDelete(ticket)}
        />
      ) : null}
    </ListRow>
  );
}

function TicketAddMenu({
  agents,
  onImportLogin,
  onAddKey,
}: {
  agents: TicketAddMenuAgent[];
  onImportLogin?: (agentId: AgentId) => void;
  onAddKey?: (agentId: AgentId) => void;
}) {
  const { t } = useI18n();
  const [open, setOpen] = React.useState(false);

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
        <DropdownMenuLabel>{t('connections.list.addAgent')}</DropdownMenuLabel>
        {agents.length === 0 ? (
          <DropdownMenuItem disabled>{t('connections.list.noAddAgent')}</DropdownMenuItem>
        ) : (
          agents.map((agent) => (
            <DropdownMenuSub key={agent.id}>
              <DropdownMenuSubTrigger className="justify-between gap-2">
                <span className="flex min-w-0 items-center gap-2">
                  <AgentDot agentId={agent.id} size="sm" title={null} />
                  <span className="truncate">{agent.name}</span>
                </span>
                <ChevronRight className="h-3.5 w-3.5 shrink-0 text-muted" />
              </DropdownMenuSubTrigger>
              <DropdownMenuSubContent
                className="min-w-[10rem]"
              >
                {agent.actions.map((action) => (
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
                ))}
              </DropdownMenuSubContent>
            </DropdownMenuSub>
          ))
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

export function TicketWalletList({
  wallet,
  loading,
  highlightAgentId,
  initialFilter = 'all',
  onConnectTicket,
  extrasForTicket,
  onEditTicket,
  onDeleteTicket,
  onAddKey,
  onImportLogin,
  installedAgentIds,
}: {
  wallet: TicketWallet | null;
  loading?: boolean;
  highlightAgentId?: AgentId | null;
  initialFilter?: TicketWalletFilter;
  onConnectTicket: (ticket: TicketView) => void;
  extrasForTicket?: (ticket: TicketView) => TicketDetailExtras | null;
  onEditTicket: (ticket: TicketView) => void;
  onDeleteTicket: (ticket: TicketView) => void;
  onAddKey?: (agentId: AgentId) => void;
  onImportLogin?: (agentId: AgentId) => void;
  installedAgentIds?: readonly AgentId[];
}) {
  const { t } = useI18n();
  const [filter, setFilter] = React.useState<TicketWalletFilter>(initialFilter);
  const [query, setQuery] = React.useState('');

  React.useEffect(() => {
    setFilter(initialFilter);
  }, [initialFilter]);

  const tickets = wallet?.tickets ?? [];
  const counts = React.useMemo(() => countTicketsByFilter(tickets), [tickets]);
  const rows = React.useMemo(() => {
    if (!wallet) return [];
    try {
      return buildTicketWalletRows(wallet, {
        filter,
        query,
        highlightAgentId: highlightAgentId ?? null,
        t,
      });
    } catch {
      return [];
    }
  }, [wallet, filter, query, highlightAgentId, t]);
  const addAgents = React.useMemo(
    () => buildTicketAddMenu(installedAgentIds),
    [installedAgentIds],
  );

  const renderAddMenu = () => (
    <TicketAddMenu
      agents={addAgents}
      onImportLogin={onImportLogin}
      onAddKey={onAddKey}
    />
  );

  return (
    <div>
      <div className={cn(pageRhythm.chromeRow, 'flex-wrap justify-between gap-2')}>
        <SegmentedControl
          value={filter}
          onChange={setFilter}
          aria-label={t('connections.list.filterAria')}
          options={TICKET_WALLET_FILTERS.map((f) => ({
            value: f.value,
            label: ticketWalletFilterLabel(f.value, t),
            count: counts[f.value],
          }))}
        />
        <div className="flex items-center gap-2">
          <SearchField
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={t('connections.list.searchPlaceholder')}
            className="w-44"
            aria-label={t('connections.list.searchAria')}
          />
          {renderAddMenu()}
        </div>
      </div>

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
                setFilter('all');
                setQuery('');
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
              onConnect={onConnectTicket}
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
