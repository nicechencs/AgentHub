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
import { agentDisplayName, resolveAgentMeta } from '@/config/agents';
import type { TicketView, TicketWallet } from '@/lib/backend/contracts/ticket';
import {
  ticketCredentialClassLabel,
  ticketSurfaceLabel,
} from '@/lib/backend/contracts/ticket';
import type { AgentId } from '@/lib/types';
import { cn } from '@/lib/utils';
import {
  buildTicketAddMenu,
  dispatchTicketAddAction,
  buildTicketDetailFields,
  buildTicketWalletRows,
  countTicketsByFilter,
  formatTicketBindingDetailLines,
  ticketDetailEditLabel,
  TICKET_WALLET_FILTERS,
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

export function TicketDetailPanel({
  id,
  advanced,
  bindings,
  extras,
  editLabel,
  onEdit,
  onDelete,
}: {
  id: string;
  advanced: TicketDetailField[];
  bindings: TicketBindingDetailLine[];
  extras?: TicketDetailExtras | null;
  editLabel?: string | null;
  onEdit?: () => void;
  onDelete: () => void;
}) {
  const has7d = extras?.quota7dPct != null;
  const has5h = extras?.quota5hPct != null;
  const hasQuota = has7d || has5h;

  return (
    <Card
      id={id}
      variant="plain"
      className="mt-3 flex flex-col gap-3 bg-canvas p-3 text-xs"
    >
      <div className={cn('grid gap-3', hasQuota && 'sm:grid-cols-2')}>
        {hasQuota ? (
          <div>
            <p className="text-meta text-muted">用量</p>
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
          <p className="text-meta text-muted">用在哪</p>
          {bindings.length === 0 ? (
            <p className="mt-1.5 text-body text-secondary">还没接到任何工具</p>
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

      {advanced.length > 0 ? (
        <details>
          <summary className="cursor-pointer text-meta text-muted">更多</summary>
          <div className="mt-1.5 grid gap-1.5 text-secondary sm:grid-cols-2">
            {advanced.map((field) => (
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
        {editLabel && onEdit ? (
          <Button size="sm" variant="secondary" onClick={onEdit}>
            <Pencil className="h-3.5 w-3.5" /> {editLabel}
          </Button>
        ) : null}
        <Button
          size="sm"
          variant="dangerOutline"
          title={extras?.isCurrent ? '移入回收站；本机连接可能仍继续生效' : undefined}
          onClick={onDelete}
        >
          <Trash2 className="h-3.5 w-3.5" /> 移入回收站
        </Button>
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
  const { ticket, usageParts, highlighted } = row;
  const [expanded, setExpanded] = React.useState(false);
  const detailsId = React.useId();
  const editLabel = ticketDetailEditLabel(extras);

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
            {ticketCredentialClassLabel(ticket.credentialClass)}
          </Badge>
          <Badge variant={ticket.surface === 'unknown' ? 'accent' : 'default'}>
            {ticketSurfaceLabel(ticket.surface)}
          </Badge>
          <span className="text-meta text-secondary">
            {usageParts.map((part, index) => (
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
            <Share2 className="h-3.5 w-3.5" /> 接到…
          </Button>
          <Button
            size="sm"
            variant="ghost"
            aria-expanded={expanded}
            aria-controls={detailsId}
            onClick={() => setExpanded((open) => !open)}
          >
            详情
            <ChevronDown
              className={cn('h-3.5 w-3.5 transition-transform', expanded && 'rotate-180')}
            />
          </Button>
        </div>
      </div>
      {expanded ? (
        <TicketDetailPanel
          id={detailsId}
          advanced={buildTicketDetailFields(ticket, extras).advanced}
          bindings={formatTicketBindingDetailLines(row.bindings)}
          extras={extras}
          editLabel={editLabel}
          onEdit={editLabel ? () => onEdit(ticket) : undefined}
          onDelete={() => onDelete(ticket)}
        />
      ) : null}
    </ListRow>
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
  const [filter, setFilter] = React.useState<TicketWalletFilter>(initialFilter);
  const [query, setQuery] = React.useState('');

  React.useEffect(() => {
    setFilter(initialFilter);
  }, [initialFilter]);

  const tickets = wallet?.tickets ?? [];
  const counts = React.useMemo(() => countTicketsByFilter(tickets), [tickets]);
  const rows = React.useMemo(() => {
    if (!wallet) return [];
    return buildTicketWalletRows(wallet, {
      filter,
      query,
      highlightAgentId: highlightAgentId ?? null,
    });
  }, [wallet, filter, query, highlightAgentId]);
  const addAgents = React.useMemo(
    () => buildTicketAddMenu(installedAgentIds),
    [installedAgentIds],
  );

  const addMenu = (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button>
          <Plus className="h-4 w-4" /> 添加 <ChevronDown className="h-3.5 w-3.5" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="min-w-[12rem]">
        <DropdownMenuLabel>选择 Agent</DropdownMenuLabel>
        {addAgents.length === 0 ? (
          <DropdownMenuItem disabled>没有可添加的 Agent</DropdownMenuItem>
        ) : (
          addAgents.map((agent) => (
            <DropdownMenuSub key={agent.id}>
              <DropdownMenuSubTrigger className="justify-between gap-2">
                <span className="flex min-w-0 items-center gap-2">
                  <AgentDot agentId={agent.id} size="sm" title={null} />
                  <span className="truncate">{agent.name}</span>
                </span>
                <ChevronRight className="h-3.5 w-3.5 shrink-0 text-muted" />
              </DropdownMenuSubTrigger>
              <DropdownMenuSubContent className="min-w-[10rem]">
                {agent.actions.map((action) => (
                  <DropdownMenuItem
                    key={action.kind}
                    disabled={action.kind === 'import-login' ? !onImportLogin : !onAddKey}
                    onSelect={() =>
                      dispatchTicketAddAction(action.kind, agent.id, {
                        onImportLogin,
                        onAddKey,
                      })
                    }
                  >
                    {action.label}
                  </DropdownMenuItem>
                ))}
              </DropdownMenuSubContent>
            </DropdownMenuSub>
          ))
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  );

  return (
    <div>
      <div className={cn(pageRhythm.chromeRow, 'flex-wrap justify-between gap-2')}>
        <SegmentedControl
          value={filter}
          onChange={setFilter}
          aria-label="登录类型筛选"
          options={TICKET_WALLET_FILTERS.map((f) => ({
            value: f.value,
            label: f.label,
            count: counts[f.value],
          }))}
        />
        <div className="flex items-center gap-2">
          <SearchField
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="搜索登录或用途"
            className="w-44"
            aria-label="搜索登录"
          />
          {addMenu}
        </div>
      </div>

      {loading && !wallet ? <ListSkeleton rows={4} /> : null}

      {wallet && tickets.length === 0 ? (
        <EmptyState
          icon={KeyRound}
          title="钱包还没有登录"
          description="导入官方登录态或添加 API Key，再接到其他 Agent。"
          action={addMenu}
        />
      ) : null}

      {wallet && tickets.length > 0 && rows.length === 0 ? (
        <EmptyState
          icon={KeyRound}
          title="没有匹配的登录"
          description="请改用其他筛选或清空搜索。"
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
              显示全部
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
          钱包 · {tickets.length} 份登录
          {highlightAgentId
            ? ` · 已高亮 ${agentDisplayName(highlightAgentId)} 的当前绑定`
            : ''}
        </p>
      ) : null}
    </div>
  );
}
