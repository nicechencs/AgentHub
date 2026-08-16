import { Loader2, PanelLeftClose, Plus, Trash2 } from 'lucide-react';
import { AgentDot } from '@/components/shared/AgentDot';
import { SearchField } from '@/components/shared/SearchField';
import { Button } from '@/components/ui/button';
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
import type { Conversation } from '@/lib/types';
import { cn } from '@/lib/utils';
import { relativeTime } from './chat-format';
import {
  conversationTitle,
  cwdShortName,
  visibleAgentDots,
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
  sendingConversationId,
  agentsReady,
  hasUsableAgent,
  deleteConfirmId,
  onToggleRail,
  onNewChat,
  onFocus,
  onRequestDelete,
  onCancelDelete,
  onConfirmDelete,
}: {
  open: boolean;
  listLoading: boolean;
  groups: ConversationDayGroup[];
  conversations: Conversation[];
  filteredCount: number;
  query: string;
  onQueryChange: (q: string) => void;
  activeId: string | null;
  sendingConversationId: string | null;
  agentsReady: boolean;
  hasUsableAgent: boolean;
  deleteConfirmId: string | null;
  onToggleRail: () => void;
  onNewChat: () => void;
  onFocus: (id: string) => void;
  onRequestDelete: (id: string) => void;
  onCancelDelete: () => void;
  onConfirmDelete: () => void;
}) {
  const pending = conversations.find((c) => c.id === deleteConfirmId) ?? null;

  return (
    <aside
      className={cn(
        'flex shrink-0 flex-col border-r border-border bg-canvas transition-[width] duration-200',
        open ? 'w-60' : 'w-0 overflow-hidden border-r-0',
      )}
    >
      <div className="flex items-center gap-1.5 p-2">
        <Hint label="收起历史">
          <button
            type="button"
            className="flex h-7 w-7 shrink-0 items-center justify-center rounded-btn text-muted hover:bg-hover hover:text-primary"
            onClick={onToggleRail}
            aria-label="收起历史"
          >
            <PanelLeftClose className="h-4 w-4" />
          </button>
        </Hint>
        <Hint label={agentsReady && !hasUsableAgent ? '请先安装或取消隐藏 Agent' : undefined}>
          <Button
            className="min-w-0 flex-1 justify-start gap-1.5"
            size="sm"
            variant="secondary"
            disabled={agentsReady && !hasUsableAgent}
            onClick={onNewChat}
          >
            <Plus className="h-3.5 w-3.5" />
            新建对话
          </Button>
        </Hint>
      </div>
      <div className="px-2 pb-2">
        <SearchField
          placeholder="搜索对话"
          value={query}
          onChange={(e) => onQueryChange(e.target.value)}
          aria-label="搜索对话"
        />
      </div>
      <div className="flex-1 overflow-y-auto px-1.5 pb-3">
        {listLoading ? (
          <div className="space-y-2 px-1 pt-1">
            {Array.from({ length: 5 }).map((_, i) => (
              <Skeleton key={i} className="h-10 w-full rounded-btn" />
            ))}
          </div>
        ) : conversations.length === 0 ? (
          <div className="px-2 py-4 text-center">
            <p className="text-xs text-muted">暂无对话</p>
            <p className="mt-1 text-2xs text-muted">点上方「新建对话」开始</p>
          </div>
        ) : filteredCount === 0 ? (
          <div className="px-2 py-4 text-center">
            <p className="text-xs text-muted">没有匹配的对话</p>
          </div>
        ) : (
          groups.map((group) => (
            <div key={group.key} className="mb-2">
              <div className="px-2 pb-1 pt-1.5 text-2xs font-medium uppercase tracking-wide text-muted">
                {group.label}
              </div>
              {group.items.map((c) => {
                const selected = activeId === c.id;
                const dots = visibleAgentDots(c.agentIds);
                const sending = sendingConversationId === c.id;
                return (
                  <Hint
                    key={c.id}
                    label={`${c.cwd || '未设目录'} · ${relativeTime(c.updatedAt)}`}
                    side="right"
                  >
                    <div
                      className={cn(
                        'group mb-0.5 flex items-center rounded-btn',
                        selected ? 'bg-active' : 'hover:bg-hover',
                      )}
                    >
                      <button
                        type="button"
                        onClick={() => onFocus(c.id)}
                        className={cn(
                          'min-w-0 flex-1 px-2 py-1.5 text-left text-sm',
                          selected ? 'font-medium text-primary' : 'text-secondary',
                        )}
                      >
                        <span className="flex items-center gap-1.5">
                          <span className="truncate">{conversationTitle(c.title)}</span>
                          {sending && (
                            <Loader2 className="h-3 w-3 shrink-0 animate-spin text-muted" />
                          )}
                        </span>
                        <span className="mt-0.5 flex items-center gap-1.5 text-xs text-muted">
                          <span className="inline-flex items-center gap-0.5">
                            {dots.shown.map((id) => (
                              <AgentDot key={id} agentId={id} size="sm" title={null} />
                            ))}
                            {dots.extra > 0 && <span>+{dots.extra}</span>}
                          </span>
                          <span className="truncate">{cwdShortName(c.cwd)}</span>
                        </span>
                      </button>
                      <Hint label="删除">
                        <button
                          type="button"
                          className="mr-1 rounded-btn p-1 opacity-0 transition-opacity hover:bg-panel group-hover:opacity-100 focus-visible:opacity-100 group-focus-within:opacity-100"
                          aria-label="删除"
                          onClick={() => onRequestDelete(c.id)}
                        >
                          <Trash2 className="h-3.5 w-3.5 text-muted hover:text-danger" />
                        </button>
                      </Hint>
                    </div>
                  </Hint>
                );
              })}
            </div>
          ))
        )}
      </div>

      <Dialog open={Boolean(deleteConfirmId)} onOpenChange={(next) => !next && onCancelDelete()}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>删除「{conversationTitle(pending?.title ?? '')}」？</DialogTitle>
            <DialogDescription>
              消息记录将一并删除；正在生成的回复会先停止。
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="secondary" onClick={onCancelDelete}>
              取消
            </Button>
            <Button variant="danger" onClick={onConfirmDelete}>
              确认删除
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </aside>
  );
}
