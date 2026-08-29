import type { RefObject } from 'react';
import { Loader2 } from 'lucide-react';
import { AgentLogo } from '@/components/shared/AgentLogo';
import { ErrorState } from '@/components/shared/ErrorState';
import { useI18n } from '@/components/shared/LanguageProvider';
import { StatusPin } from '@/components/shared/StatusPin';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { ListSkeleton } from '@/components/ui/skeleton';
import { agentDisplayName } from '@/config/agents';
import { processKey, type ProcessMap } from '@/lib/chat-process';
import type { ChatMessageStatus, Conversation } from '@/lib/types';
import { cn } from '@/lib/utils';
import type { TranslateFn } from '@/lib/i18n';
import { formatDurationMs, type TurnGroup } from './chat-format';
import {
  agentPickerLabel,
  chatTranscriptSurfaceClass,
  turnComparisonChips,
} from './chat-model';
import { ChatMessageBubble } from './ChatMessageBubble';

export function ChatTranscript({
  active,
  turns,
  processMap,
  listLoading,
  messagesLoading,
  messagesError,
  onRetryMessages,
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
  messagesError?: unknown;
  onRetryMessages?: () => void;
  sending: boolean;
  retryDisabled: boolean;
  scrollRef: RefObject<HTMLDivElement>;
  bottomRef: RefObject<HTMLDivElement>;
  onScroll: () => void;
  onRetry: () => void;
}) {
  const { t } = useI18n();
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
    <div
      ref={scrollRef}
      onScroll={onScroll}
      className="min-h-0 flex-1 overflow-y-auto bg-canvas"
    >
      {messagesLoading && turns.length === 0 ? (
        <div className="flex h-full flex-col justify-center p-6">
          <ListSkeleton rows={3} className="mx-auto w-full max-w-2xl" />
        </div>
      ) : messagesError && turns.length === 0 ? (
        <div className="flex h-full items-center justify-center p-6">
          <ErrorState
            error={messagesError}
            title={t('chat.transcript.loadFailed')}
            onRetry={onRetryMessages ?? (() => {})}
          />
        </div>
      ) : turns.length === 0 ? (
        <div className="flex h-full flex-col items-center justify-center px-6 py-10">
          <div className="text-center">
            <p className="text-title font-semibold tracking-tight text-primary">{t('chat.transcript.start')}</p>
            <p className="mt-2 max-w-md text-body text-muted">
              {t('chat.transcript.firstMessage', { agent: agentPickerLabel(t, active) })}
            </p>
          </div>
        </div>
      ) : (
        <div
          className={cn(
            'min-h-full rounded-composer',
            chatTranscriptSurfaceClass(turns.length > 0),
          )}
        >
          <div className={cn('space-y-6 py-4', pageRhythm.chatChromeX)}>
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
  const { t } = useI18n();
  return (
    <div className="flex flex-wrap items-center gap-2 text-meta text-muted">
      <span>{t('chat.transcript.turnAgents', { n: chips.length })}</span>
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

function chipStatusLabel(status: ChatMessageStatus, t: TranslateFn): string {
  switch (status) {
    case 'running':
      return t('chat.transcript.generating');
    case 'cancelled':
      return t('chat.transcript.cancelled');
    case 'ok':
      return t('chat.transcript.success');
    case 'timeout':
      return t('chat.transcript.timeout');
    case 'skipped':
      return t('chat.transcript.skipped');
    default:
      return t('chat.transcript.failed');
  }
}

function ChipStatus({ status }: { status: ChatMessageStatus }) {
  const { t } = useI18n();
  const label = chipStatusLabel(status, t);
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
      ? 'success'
      : status === 'failed' || status === 'timeout'
        ? 'danger'
        : 'muted';
  return (
    <span className="inline-flex" aria-label={label}>
      <StatusPin tone={tone} size="sm" />
    </span>
  );
}
