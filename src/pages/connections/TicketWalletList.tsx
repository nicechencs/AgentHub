/**
 * Global ticket wallet list UI (Connections).
 * Data from listTicketWallet; per-row「接到…」always for true tickets.
 * 「详情」is a read-only expand; edit/delete stay secondary actions inside it.
 */
import * as React from 'react';
import { Link } from 'react-router-dom';
import { ChevronDown, KeyRound, Pencil, Plus, Share2, Search, Trash2 } from 'lucide-react';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { DetailRow } from '@/components/shared/DetailRow';
import { EmptyState } from '@/components/shared/EmptyState';
import { ListRow } from '@/components/shared/ListRow';
import { QuotaBar } from '@/components/shared/QuotaBar';
import { SegmentedControl } from '@/components/shared/SegmentedControl';
import { StatusDot } from '@/components/shared/StatusDot';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import { Input } from '@/components/ui/input';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { agentDisplayName } from '@/config/agents';
import type { TicketView, TicketWallet } from '@/lib/backend/contracts/ticket';
import {
  ticketCredentialClassLabel,
  ticketSurfaceLabel,
} from '@/lib/backend/contracts/ticket';
import type { AgentId } from '@/lib/types';
import { cn } from '@/lib/utils';
import {
  buildTicketDetailFields,
  buildTicketWalletRows,
  countTicketsByFilter,
  formatTicketBindingDetailLines,
  ticketDetailEditLabel,
  TICKET_WALLET_FILTERS,
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
  fields,
  bindingLines,
  extras,
  editLabel,
  onEdit,
  onDelete,
}: {
  id: string;
  fields: TicketDetailField[];
  bindingLines: string[];
  extras?: TicketDetailExtras | null;
  editLabel?: string | null;
  onEdit?: () => void;
  onDelete: () => void;
}) {
  return (
    <Card
      id={id}
      variant="plain"
      className="mt-3 flex flex-col gap-2.5 bg-canvas p-3 text-xs"
    >
      <div className="grid gap-1.5 text-secondary sm:grid-cols-2">
        {fields.map((field) => (
          <DetailRow
            key={`${field.label}:${field.value}`}
            label={field.label}
            value={field.value}
            mono={field.mono}
          />
        ))}
        {extras?.authLabel ? (
          <span className="inline-flex items-center gap-1.5 sm:col-span-2">
            登录态 {extras.authStatus ? <StatusDot status={extras.authStatus} /> : null}
            <span className="text-xs text-secondary">{extras.authLabel}</span>
          </span>
        ) : null}
      </div>

      <div>
        <p className="text-2xs text-muted">正用于</p>
        {bindingLines.length === 0 ? (
          <p className="mt-1 text-secondary">未绑定任何 Agent</p>
        ) : (
          <ul className="mt-1 space-y-0.5 text-secondary">
            {bindingLines.map((line) => (
              <li key={line}>{line}</li>
            ))}
          </ul>
        )}
      </div>

      {extras?.quota5hPct != null || extras?.quota7dPct != null ? (
        <div className="flex flex-wrap items-center gap-x-6 gap-y-1.5">
          <QuotaBar label="5h" pct={extras.quota5hPct} resetIn={extras.quotaResetIn} />
          <QuotaBar label="7d" pct={extras.quota7dPct} resetIn={extras.quota7dResetIn} />
        </div>
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
      className={cn('p-3', highlighted && 'ring-1 ring-accent/40')}
    >
      <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
        <div className="flex min-w-0 flex-1 items-center gap-2">
          <span
            className={cn('text-xs', highlighted ? 'text-success' : 'text-muted')}
            aria-hidden
          >
            {highlighted ? '●' : '○'}
          </span>
          <span className="truncate text-sm font-medium" title={ticket.label}>
            {ticket.label}
          </span>
          <Badge variant={credentialBadgeVariant(ticket.credentialClass)}>
            {ticketCredentialClassLabel(ticket.credentialClass)}
          </Badge>
          <Badge variant={ticket.surface === 'unknown' ? 'accent' : 'default'}>
            {ticketSurfaceLabel(ticket.surface)}
          </Badge>
          <span className="truncate text-2xs text-muted">
            {agentDisplayName(ticket.agentId)}
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
      <p className="mt-1 pl-5 text-2xs text-secondary">
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
      </p>
      {expanded ? (
        <TicketDetailPanel
          id={detailsId}
          fields={buildTicketDetailFields(ticket, extras)}
          bindingLines={formatTicketBindingDetailLines(row.bindings)}
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
  addAgentId,
  installedAgentIds,
  onPickAddAgent,
}: {
  wallet: TicketWallet | null;
  loading?: boolean;
  highlightAgentId?: AgentId | null;
  initialFilter?: TicketWalletFilter;
  onConnectTicket: (ticket: TicketView) => void;
  extrasForTicket?: (ticket: TicketView) => TicketDetailExtras | null;
  onEditTicket: (ticket: TicketView) => void;
  onDeleteTicket: (ticket: TicketView) => void;
  onAddKey?: () => void;
  onImportLogin?: () => void;
  addAgentId?: AgentId | null;
  installedAgentIds?: readonly AgentId[];
  onPickAddAgent?: (id: AgentId) => void;
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

  const addMenu = (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button>
          <Plus className="h-4 w-4" /> 添加 <ChevronDown className="h-3.5 w-3.5" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="min-w-[14rem]">
        {installedAgentIds && installedAgentIds.length > 0 && onPickAddAgent ? (
          <>
            <div className="px-2 py-1.5 text-2xs text-muted">添加到 Agent</div>
            {installedAgentIds.map((id) => (
              <DropdownMenuItem
                key={id}
                onSelect={() => onPickAddAgent(id)}
                className={addAgentId === id ? 'bg-active' : undefined}
              >
                {agentDisplayName(id)}
                {addAgentId === id ? ' · 当前' : ''}
              </DropdownMenuItem>
            ))}
            <div className="my-1 h-px bg-border" />
          </>
        ) : null}
        <DropdownMenuItem disabled={!onImportLogin} onSelect={() => onImportLogin?.()}>
          导入当前登录态
        </DropdownMenuItem>
        <DropdownMenuItem disabled={!onAddKey} onSelect={() => onAddKey?.()}>
          添加 API Key
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );

  return (
    <div>
      <div className={cn(pageRhythm.chromeRow, 'flex-wrap justify-between gap-2')}>
        <SegmentedControl
          value={filter}
          onChange={setFilter}
          aria-label="票类型筛选"
          options={TICKET_WALLET_FILTERS.map((f) => ({
            value: f.value,
            label: f.label,
            count: counts[f.value],
          }))}
        />
        <div className="flex items-center gap-2">
          <div className="relative">
            <Search className="pointer-events-none absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted" />
            <Input
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="搜票 / 搜用途"
              className="h-8 w-44 pl-7 text-xs"
              aria-label="搜索票"
            />
          </div>
          {addMenu}
        </div>
      </div>

      {loading && !wallet ? (
        <p className="text-xs text-muted">正在加载钱包…</p>
      ) : null}

      {wallet && tickets.length === 0 ? (
        <EmptyState
          icon={KeyRound}
          title="钱包还没有票"
          description="导入官方登录态或添加 API Key，再接到其他 Agent。"
          action={addMenu}
        />
      ) : null}

      {wallet && tickets.length > 0 && rows.length === 0 ? (
        <EmptyState
          icon={KeyRound}
          title="没有匹配的票"
          description="请改用其他筛选或清空搜索。"
          actionLabel="显示全部"
          onAction={() => {
            setFilter('all');
            setQuery('');
          }}
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
        <p className="mt-3 text-2xs text-muted">
          钱包 · {tickets.length} 张票
          {highlightAgentId
            ? ` · 已高亮 ${agentDisplayName(highlightAgentId)} 的当前绑定`
            : ''}
        </p>
      ) : null}
    </div>
  );
}
