import type { RefObject } from 'react';
import { Loader2 } from 'lucide-react';
import { AgentLogo } from '@/components/shared/AgentLogo';
import { ListSkeleton } from '@/components/ui/skeleton';
import { agentDisplayName } from '@/config/agents';
import { processKey, type ProcessMap } from '@/lib/chat-process';
import type { ChatMessageStatus, Conversation } from '@/lib/types';
import { cn } from '@/lib/utils';
import { formatDurationMs, type TurnGroup } from './chat-format';
import { agentPickerLabel, turnComparisonChips } from './chat-model';
import { ChatMessageBubble } from './ChatMessageBubble';

export function ChatTranscript({
  active,
  turns,
  processMap,
  listLoading,
  messagesLoading,
  sending,
  retryDisabled,
  scrollRef,
  bottomRef,
  onScroll,
  onRetry,
}: {
  active: Conversation | null;
  turns: TurnGroup[];
  processMap: ProcessMap;
  listLoading: boolean;
  messagesLoading: boolean;
  sending: boolean;
  retryDisabled: boolean;
  scrollRef: RefObject<HTMLDivElement>;
  bottomRef: RefObject<HTMLDivElement>;
  onScroll: () => void;
  onRetry: () => void;
}) {
  if (listLoading && !active) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center gap-3 p-6">
        <ListSkeleton rows={4} className="w-full max-w-2xl" />
      </div>
    );
  }

  if (!active) return <div className="min-h-0 flex-1" />;

  const lastTurn = turns[turns.length - 1]?.turn;

  return (
    <div ref={scrollRef} onScroll={onScroll} className="min-h-0 flex-1 overflow-y-auto">
      {messagesLoading && turns.length === 0 ? (
        <div className="flex h-full flex-col justify-center p-6">
          <ListSkeleton rows={3} className="mx-auto w-full max-w-2xl" />
        </div>
      ) : turns.length === 0 ? (
        <div className="flex h-full flex-col items-center justify-center px-6 py-10">
          <div className="text-center">
            <p className="text-xl font-semibold tracking-tight text-primary">开始对话</p>
            <p className="mt-2 max-w-md text-sm text-muted">
              向 {agentPickerLabel(active)} 发送第一条消息；多选 Agent 可同轮对比
            </p>
          </div>
        </div>
      ) : (
        <div className="mx-auto w-full max-w-3xl space-y-6 px-4 py-6">
          {turns.map((g) => {
            const chips = g.agents.length >= 2 ? turnComparisonChips(g.agents) : [];
            return (
              <div key={g.turn} className="space-y-4">
                {g.user && (
                  <ChatMessageBubble
                    message={g.user}
                    isLastTurn={g.turn === lastTurn}
                    multiAgent={g.agents.length > 1}
                    retryDisabled={retryDisabled || sending}
                    onRetry={onRetry}
                  />
                )}
                {chips.length > 0 && (
                  <ComparisonBar chips={chips} />
                )}
                {g.agents.map((m) => {
                  const agent = m.agentId ?? 'claude';
                  return (
                    <ChatMessageBubble
                      key={m.id}
                      message={m}
                      process={processMap[processKey(m.turn, agent)]}
                      isLastTurn={g.turn === lastTurn}
                      multiAgent={g.agents.length > 1}
                      retryDisabled={retryDisabled || sending}
                      onRetry={onRetry}
                    />
                  );
                })}
              </div>
            );
          })}
          <div ref={bottomRef} />
        </div>
      )}
    </div>
  );
}

function ComparisonBar({
  chips,
}: {
  chips: Array<{
    agentId: string;
    status: ChatMessageStatus;
    durationMs: number;
    messageId: string;
  }>;
}) {
  return (
    <div className="flex flex-wrap items-center gap-2 text-xs text-muted">
      <span>本轮 {chips.length} 个 Agent</span>
      {chips.map((chip) => (
        <button
          key={chip.messageId}
          type="button"
          className="inline-flex items-center gap-1.5 rounded-btn border border-border bg-subtle px-2 py-1 text-secondary hover:bg-hover"
          onClick={() => {
            document
              .getElementById(`chat-msg-${chip.messageId}`)
              ?.scrollIntoView({ behavior: 'smooth', block: 'center' });
          }}
        >
          <AgentLogo agentId={chip.agentId} size="sm" />
          <span>{agentDisplayName(chip.agentId)}</span>
          <ChipStatus status={chip.status} />
          {chip.durationMs > 0 && <span>{formatDurationMs(chip.durationMs)}</span>}
        </button>
      ))}
    </div>
  );
}

function chipStatusLabel(status: ChatMessageStatus): string {
  switch (status) {
    case 'running':
      return '生成中';
    case 'cancelled':
      return '已取消';
    case 'ok':
      return '成功';
    case 'timeout':
      return '超时';
    case 'skipped':
      return '已跳过';
    default:
      return '失败';
  }
}

function ChipStatus({ status }: { status: ChatMessageStatus }) {
  const label = chipStatusLabel(status);
  if (status === 'running') {
    return (
      <span className="inline-flex" aria-label={label}>
        <Loader2 className="h-3 w-3 animate-spin text-muted" aria-hidden />
        <span className="sr-only">{label}</span>
      </span>
    );
  }
  const tone =
    status === 'ok'
      ? 'bg-success'
      : status === 'failed' || status === 'timeout'
        ? 'bg-danger'
        : 'bg-muted';
  return (
    <span className={cn('inline-block h-1.5 w-1.5 rounded-full', tone)} aria-label={label}>
      <span className="sr-only">{label}</span>
    </span>
  );
}
