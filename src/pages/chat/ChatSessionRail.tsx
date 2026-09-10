import { useEffect, useRef } from 'react';
import { Loader2, PanelLeftClose, Plus, Trash2 } from 'lucide-react';
import { AgentLogo } from '@/components/shared/AgentLogo';
import { NavResizeHandle } from '@/components/layout/NavResizeHandle';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { CHAT_RAIL_WIDTH } from '@/components/layout/sidebar-width-model';
import { useNavWidth } from '@/components/layout/use-sidebar-width';
import { useI18n } from '@/components/shared/LanguageProvider';
import { SearchField } from '@/components/shared/SearchField';
import { Button } from '@/components/ui/button';
import { EnterKeyMark } from '@/components/ui/shortcut-kbd';
import { dialogEnterShouldConfirm } from '@/lib/dialog-enter';
import { Skeleton } from '@/components/ui/skeleton';
import { Hint } from '@/components/ui/tooltip';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { StorageKey } from '@/lib/storage-key';
import type { Conversation } from '@/lib/types';
import { cn } from '@/lib/utils';
import {
  conversationRailHintView,
  conversationRailMarkColor,
  conversationRailSelectedFill,
  conversationTitle,
  cwdShortName,
  isBlankConversationDraft,
  type ConversationDayGroup,
} from './chat-model';

export function ChatSessionRail({
  open,
  listLoading,
  groups,
  conversations,
  filteredCount,
  query,
  onQueryChange,
  activeId,
  sendingConversationIds,
  agentsReady,
  hasUsableAgent,
  deleteConfirmId,
  onToggleRail,
  onNewChat,
  onFocus,
  onRequestDelete,
  onCancelDelete,
  onConfirmDelete,
  searchFocusNonce = 0,
  historyRevealNonce = 0,
}: {
  open: boolean;
  listLoading: boolean;
  groups: ConversationDayGroup[];
  conversations: Conversation[];
  filteredCount: number;
  query: string;
  onQueryChange: (q: string) => void;
  activeId: string | null;
  sendingConversationIds: readonly string[];
  agentsReady: boolean;
  hasUsableAgent: boolean;
  deleteConfirmId: string | null;
  onToggleRail: () => void;
  onNewChat: () => void;
  onFocus: (id: string) => void;
  onRequestDelete: (id: string) => void;
  onCancelDelete: () => void;
  onConfirmDelete: () => void;
  searchFocusNonce?: number;
  historyRevealNonce?: number;
}) {
  const { t } = useI18n();
  const width = useNavWidth({
    collapsed: !open,
    storageKey: StorageKey.chatRailWidth,
    policy: CHAT_RAIL_WIDTH,
  });
  const pending = conversations.find((c) => c.id === deleteConfirmId) ?? null;
  const railRef = useRef<HTMLElement>(null);
  const searchInputRef = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (!open || !searchFocusNonce) return;
    const timer = window.setTimeout(() => {
      searchInputRef.current?.focus();
      searchInputRef.current?.select();
    }, 0);
    return () => window.clearTimeout(timer);
  }, [open, searchFocusNonce]);
  useEffect(() => {
    if (!open || !historyRevealNonce) return;
    const timer = window.setTimeout(() => {
      const selected = railRef.current?.querySelector<HTMLElement>(
        '[data-session-id][data-selected="true"]',
      );
      const fallback = railRef.current?.querySelector<HTMLElement>('[data-session-id]');
      (selected ?? fallback)?.focus();
    }, 0);
    return () => window.clearTimeout(timer);
  }, [open, historyRevealNonce]);

  return (
    <>
    <aside
      ref={railRef}
      className={cn(
        'flex shrink-0 flex-col overflow-hidden bg-canvas',
        width.widthTransition,
        open && 'rounded-card border border-border',
      )}
      style={{ width: width.width }}
      data-help="chat-rail"
    >
      <div
        className={cn(
          'flex shrink-0 items-center justify-between gap-1 border-b border-border px-2',
          pageRhythm.topChrome,
        )}
      >
        <h2 className="min-w-0 truncate text-body font-semibold text-primary">
          {t('chat.rail.historyTitle')}
        </h2>
        <Button
          type="button"
          size="icon"
          variant="ghost"
          className="shrink-0 text-muted"
          onClick={onToggleRail}
          title={t('chat.rail.collapseHistory')}
          aria-label={t('chat.rail.collapseHistory')}
        >
          <PanelLeftClose className="h-4 w-4" />
        </Button>
      </div>
      <div className="shrink-0 px-2 pt-2">
        <Hint label={agentsReady && !hasUsableAgent ? t('chat.rail.newChatDisabled') : undefined}>
          <Button
            className="w-full justify-start gap-1.5"
            size="sm"
            variant="default"
            disabled={agentsReady && !hasUsableAgent}
            data-help="chat-new"
            aria-keyshortcuts="Control+N"
            onClick={onNewChat}
          >
            <Plus className="h-3.5 w-3.5" />
            {t('chat.rail.newChat')}
          </Button>
        </Hint>
      </div>
      <div className="px-2 pb-2 pt-3">
        <SearchField
          inputRef={searchInputRef}
          placeholder={t('chat.rail.searchPlaceholder')}
          value={query}
          onChange={(e) => onQueryChange(e.target.value)}
          aria-label={t('chat.rail.searchAria')}
        />
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto px-1.5 pb-3">
        {listLoading ? (
          <div className="space-y-2 px-1 pt-1">
            {Array.from({ length: 5 }).map((_, i) => (
              <Skeleton key={i} className="h-8 w-full rounded-btn" />
            ))}
          </div>
        ) : conversations.length === 0 ? (
          <div className="px-2 py-4 text-center">
            <p className="text-meta text-muted">{t('chat.rail.empty')}</p>
            <p className="mt-1 text-meta text-muted">{t('chat.rail.emptyHint')}</p>
          </div>
        ) : filteredCount === 0 ? (
          <div className="px-2 py-4 text-center">
            <p className="text-meta text-muted">{t('chat.rail.noMatch')}</p>
          </div>
        ) : (
          groups.map((group) => (
            <div key={group.key} className="mb-2">
              <div className={cn('px-2 pb-1 pt-1.5', pageRhythm.sectionEyebrow)}>
                {group.label}
              </div>
              {group.items.map((c) => {
                const selected = activeId === c.id;
                const sending = sendingConversationIds.includes(c.id);
                return (
                  <div
                    key={c.id}
                    className={cn(
                      'group relative mb-0.5 flex items-center rounded-btn',
                      !selected && 'hover:bg-hover',
                    )}
                    style={
                      selected
                        ? { backgroundColor: conversationRailSelectedFill(c.agentIds) }
                        : undefined
                    }
                  >
                    {selected ? (
                      <span
                        aria-hidden
                        className="absolute inset-y-1.5 left-0 w-0.5 rounded-full"
                        style={{ backgroundColor: conversationRailMarkColor(c.agentIds) }}
                      />
                    ) : null}
                    <Hint
                      label={<ConversationRailHintLabel conversation={c} />}
                      side="right"
                      contentClassName="min-w-0 whitespace-normal break-words"
                    >
                      <button
                        type="button"
                        data-session-id={c.id}
                        data-selected={selected ? 'true' : undefined}
                        aria-current={selected ? 'true' : undefined}
                        onClick={() => onFocus(c.id)}
                        className={cn(
                          'flex min-w-0 flex-1 items-center gap-1.5 px-2 py-1.5 text-left text-body',
                          selected ? 'font-medium text-primary' : 'text-secondary',
                        )}
                      >
                        {c.agentIds[0] ? (
                          <AgentLogo agentId={c.agentIds[0]} size="sm" hint={false} />
                        ) : null}
                        <span className="min-w-0 flex-1">
                          <span className="block truncate" data-help="chat-session-title">
                            {conversationTitle(t, c.title)}
                          </span>
                          <span className="block truncate text-meta text-muted">
                            {cwdShortName(c.cwd, t)}
                            {isBlankConversationDraft(c) ? ` · ${t('chat.rail.draft')}` : ''}
                          </span>
                        </span>
                        {sending ? (
                          <Loader2 className="h-3.5 w-3.5 shrink-0 animate-spin text-muted" />
                        ) : null}
                      </button>
                    </Hint>
                    <Button
                      type="button"
                      size="icon"
                      variant="ghost"
                      className="mr-1 text-muted opacity-0 transition-opacity hover:text-danger group-hover:opacity-100 focus-visible:opacity-100 group-focus-within:opacity-100"
                      title={t('chat.rail.deleteAria')}
                      aria-label={t('chat.rail.deleteAria')}
                      onClick={() => onRequestDelete(c.id)}
                    >
                      <Trash2 className="h-3.5 w-3.5" />
                    </Button>
                  </div>
                );
              })}
            </div>
          ))
        )}
      </div>
      <Dialog open={Boolean(deleteConfirmId)} onOpenChange={(next) => !next && onCancelDelete()}>
        <DialogContent
          onKeyDown={(event) => {
            if (!dialogEnterShouldConfirm({
              key: event.key,
              shiftKey: event.shiftKey,
              isComposing: event.nativeEvent.isComposing,
              nativeEvent: event.nativeEvent,
            })) return;
            event.preventDefault();
            onConfirmDelete();
          }}
        >
          <DialogHeader>
            <DialogTitle>
              {t('chat.rail.deleteTitle', { title: conversationTitle(t, pending?.title ?? '') })}
            </DialogTitle>
            <DialogDescription>
              {t('chat.rail.deleteDesc')}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="secondary" onClick={onCancelDelete}>
              {t('common.cancel')}
            </Button>
            <Button
              variant="danger"
              aria-keyshortcuts="Enter"
              onClick={onConfirmDelete}
            >
              {t('chat.rail.confirmDelete')}
              <EnterKeyMark onAccent />
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </aside>
    {open ? <NavResizeHandle label={t('chat.rail.resize')} width={width} /> : null}
    </>
  );
}

function ConversationRailHintLabel({ conversation }: { conversation: Conversation }) {
  const { t } = useI18n();
  const hint = conversationRailHintView(conversation, t);
  return (
    <span
      className="flex min-w-0 max-w-full flex-col gap-1 whitespace-normal break-words"
      data-help="chat-session-hint"
    >
      {conversation.agentIds.length > 0 ? (
        <span className="inline-flex items-center gap-0.5">
          {conversation.agentIds.map((id) => (
            <AgentLogo key={id} agentId={id} size="sm" hint={false} />
          ))}
        </span>
      ) : null}
      {hint.title ? (
        <span
          className="min-w-0 max-w-full whitespace-pre-wrap break-all [overflow-wrap:anywhere]"
          data-help="chat-session-hint-title"
        >
          {hint.title}
        </span>
      ) : null}
      <span className="min-w-0 max-w-full whitespace-normal break-words">
        {hint.meta}
      </span>
    </span>
  );
}
