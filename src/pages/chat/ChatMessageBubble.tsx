import { Copy, Loader2 } from 'lucide-react';
import { AgentLogo } from '@/components/shared/AgentLogo';
import { useI18n } from '@/components/shared/LanguageProvider';
import { MarkdownView } from '@/components/shared/MarkdownView';
import { Hint } from '@/components/ui/tooltip';
import { useToast } from '@/components/ui/toast';
import { agentDisplayName } from '@/config/agents';
import { hasProcessDetails, processPhaseLabel } from '@/lib/chat-process';
import type { AgentProcessView } from '@/lib/chat-process';
import type { ChatMessage } from '@/lib/types';
import { cn } from '@/lib/utils';
import { formatDurationMs } from './chat-format';
import { messageStatusLabel } from './chat-model';
import { ChatProcessPanel } from './ChatProcessPanel';

export function ChatMessageBubble({
  message,
  process,
  isLastTurn,
  multiAgent,
  retryDisabled,
  onRetry,
}: {
  message: ChatMessage;
  process?: AgentProcessView;
  isLastTurn: boolean;
  multiAgent: boolean;
  retryDisabled: boolean;
  onRetry: () => void;
}) {
  if (message.role === 'user') {
    return <UserBubble message={message} />;
  }
  return (
    <AgentBubble
      message={message}
      process={process}
      isLastTurn={isLastTurn}
      multiAgent={multiAgent}
      retryDisabled={retryDisabled}
      onRetry={onRetry}
    />
  );
}

function UserBubble({ message }: { message: ChatMessage }) {
  return (
    <div className="flex justify-end">
      <div
        id={`chat-msg-${message.id}`}
        className="group relative max-w-[85%] rounded-composer bg-subtle px-4 py-2 text-body text-primary"
      >
        <MarkdownView content={message.content} variant="chat" />
        <CopyButton text={message.content} />
      </div>
    </div>
  );
}

function AgentBubble({
  message,
  process,
  isLastTurn,
  multiAgent,
  retryDisabled,
  onRetry,
}: {
  message: ChatMessage;
  process?: AgentProcessView;
  isLastTurn: boolean;
  multiAgent: boolean;
  retryDisabled: boolean;
  onRetry: () => void;
}) {
  const { t } = useI18n();
  const agent = message.agentId ?? 'claude';
  const statusText = messageStatusLabel(t, message.status, process);
  const running = message.status === 'running';
  const showRetry =
    isLastTurn &&
    (message.status === 'failed' ||
      message.status === 'cancelled' ||
      message.status === 'timeout');

  return (
    <div id={`chat-msg-${message.id}`} className="group flex gap-3">
      <AgentLogo agentId={agent} size="md" />
      <div className="relative min-w-0 flex-1 pt-0.5">
        <div className="mb-1 flex flex-wrap items-center gap-2 text-meta text-muted">
          <span className="font-medium text-secondary">{agentDisplayName(agent)}</span>
          {statusText && <span>{statusText}</span>}
          {message.durationMs > 0 && <span>{formatDurationMs(message.durationMs)}</span>}
          {showRetry && (
            <Hint
              label={
                multiAgent ? t('chat.bubble.retryAllHint') : undefined
              }
            >
              <button
                type="button"
                disabled={retryDisabled}
                onClick={onRetry}
                className="rounded-btn px-1.5 py-0.5 text-meta text-secondary hover:bg-hover hover:text-primary disabled:opacity-50"
              >
                {t('chat.bubble.retry')}
              </button>
            </Hint>
          )}
        </div>
        {hasProcessDetails(process) && process ? (
          <ChatProcessPanel
            view={process}
            messageStatus={message.status}
            durationMs={message.durationMs}
            exitCode={message.exitCode}
          />
        ) : null}
        <div className="text-body leading-relaxed text-primary">
          {message.content ? (
            <MarkdownView content={message.content} variant="chat" />
          ) : running ? (
            <span className="inline-flex items-center gap-2 text-muted">
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
              {process
                ? t('chat.bubble.generatingPhase', { phase: processPhaseLabel(process.phase, t) })
                : t('chat.bubble.generating')}
            </span>
          ) : (
            <span className="text-muted">{message.error || t('chat.bubble.noOutput')}</span>
          )}
          {message.error && message.status !== 'ok' && message.content && (
            <p className="mt-2 text-body text-danger">{message.error}</p>
          )}
        </div>
        {!running && <CopyButton text={message.content} />}
      </div>
    </div>
  );
}

function CopyButton({ text }: { text: string }) {
  const { t } = useI18n();
  const { toast } = useToast();
  if (!text) return null;
  return (
    <button
      type="button"
      aria-label={t('chat.bubble.copyAria')}
      className={cn(
        'absolute bottom-1 right-1 rounded-btn p-1 text-muted',
        'opacity-0 transition-opacity hover:bg-panel hover:text-primary',
        'group-hover:opacity-100 focus-visible:opacity-100 group-focus-within:opacity-100',
      )}
      onClick={() => {
        void navigator.clipboard.writeText(text).then(
          () => toast({ title: t('chat.bubble.copied'), variant: 'success' }),
          () => toast({ title: t('chat.bubble.copyFailed'), variant: 'danger' }),
        );
      }}
    >
      <Copy className="h-3.5 w-3.5" />
    </button>
  );
}
