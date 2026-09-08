import type { RefObject } from 'react';
import {
  AlertTriangle,
  FolderSearch,
  ListTree,
  Loader2,
  TestTube2,
  type LucideIcon,
} from 'lucide-react';
import { AgentLogo } from '@/components/shared/AgentLogo';
import { ErrorState } from '@/components/shared/ErrorState';
import { useI18n } from '@/components/shared/LanguageProvider';
import { StatusPin } from '@/components/shared/StatusPin';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { Button } from '@/components/ui/button';
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
import {
  chatStarterActions,
  chatStarterCopyKey,
  type ChatActionDef,
  type ChatStarterCopyKey,
} from './chat-actions';
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
  onOpenLocal,
  onPickStarter,
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
  onOpenLocal?: (path: string) => boolean;
  onPickStarter?: (action: ChatActionDef) => void;
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
      role="log"
      aria-live="polite"
      aria-relevant="additions text"
      aria-busy={messagesLoading || sending ? 'true' : undefined}
      className={cn('min-h-0 flex-1 overflow-x-hidden overflow-y-auto', chatTranscriptSurfaceClass)}
      data-chat-transcript
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
        <EmptyTranscriptStart
          agentLabel={agentPickerLabel(t, active)}
          sending={sending}
          onPickStarter={onPickStarter}
        />
      ) : (
        <div className="min-h-full" data-chat-transcript-surface>
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
                      localBasePath={active.cwd ?? undefined}
                      onOpenLocal={onOpenLocal}
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
                        localBasePath={active.cwd ?? undefined}
                        onOpenLocal={onOpenLocal}
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

const STARTER_ICONS: Record<ChatStarterCopyKey, LucideIcon> = {
  understand: FolderSearch,
  check: AlertTriangle,
  summarize: ListTree,
  tests: TestTube2,
};

function EmptyTranscriptStart({
  agentLabel,
  sending,
  onPickStarter,
}: {
  agentLabel: string;
  sending: boolean;
  onPickStarter?: (action: ChatActionDef) => void;
}) {
  const { t } = useI18n();
  const starters = chatStarterActions();
  return (
    <div className="flex h-full flex-col items-center justify-center px-6 py-10">
      <div className="w-full max-w-3xl text-center">
        <p className="text-title font-semibold tracking-tight text-primary">{t('chat.transcript.start')}</p>
        <p className="mt-2 text-body text-muted">
          {t('chat.transcript.firstMessage', { agent: agentLabel })}
        </p>
        {!sending ? (
          <div
            className="mt-6 grid grid-cols-1 gap-2 sm:grid-cols-2"
            role="group"
            aria-label={t('chat.transcript.startersAria')}
          >
            {starters.map((action) => {
              const key = chatStarterCopyKey(action.id);
              if (!key) return null;
              const Icon = STARTER_ICONS[key];
              const title = t(`chat.transcript.starter.${key}` as never);
              return (
                <button
                  key={action.id}
                  type="button"
                  aria-label={title}
                  className="rounded-card border border-border bg-panel p-3 text-left shadow-xs hover:bg-hover"
                  onClick={() => onPickStarter?.(action)}
                >
                  <Icon className="mb-2 h-4 w-4 text-accent" aria-hidden />
                  <p className="text-body font-medium text-primary">{title}</p>
                  <p className="mt-1 text-meta text-muted">
                    {t(`chat.transcript.starter.${key}Hint` as never)}
                  </p>
                </button>
              );
            })}
          </div>
        ) : null}
      </div>
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
        <Button
          key={chip.messageId}
          type="button"
          size="sm"
          variant="outline"
          className="gap-1.5"
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
        </Button>
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
