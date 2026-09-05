import { Loader2 } from 'lucide-react';
import { AgentLogo } from '@/components/shared/AgentLogo';
import { CopyTextButton } from '@/components/shared/CopyTextButton';
import { useI18n } from '@/components/shared/LanguageProvider';
import { MarkdownView } from '@/components/shared/MarkdownView';
import { Button } from '@/components/ui/button';
import { Hint } from '@/components/ui/tooltip';
import { agentDisplayName } from '@/config/agents';
import { hasProcessDetails, processPhaseLabel } from '@/lib/chat-process';
import type { AgentProcessView } from '@/lib/chat-process';
import type { ChatMessage } from '@/lib/types';
import { formatDurationMs, localizeChatFailure } from './chat-format';
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
        <CopyTextButton text={message.content} />
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
  const displayContent = message.content ? localizeChatFailure(message.content, t) : '';
  const displayError = message.error ? localizeChatFailure(message.error, t) : '';
  const looksFailed =
    message.status === 'failed' ||
    message.status === 'cancelled' ||
    message.status === 'timeout' ||
    (message.status === 'ok' && displayContent !== message.content);
  const statusText = messageStatusLabel(
    t,
    looksFailed && message.status === 'ok' ? 'failed' : message.status,
    process,
  );
  const running = message.status === 'running';
  const showRetry = isLastTurn && looksFailed;

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
              <Button
                type="button"
                size="sm"
                variant="outline"
                disabled={retryDisabled}
                onClick={onRetry}
              >
                {t('chat.bubble.retry')}
              </Button>
            </Hint>
          )}
        </div>
        {hasProcessDetails(process) && process ? (
          <ChatProcessPanel
            view={process}
            messageStatus={looksFailed && message.status === 'ok' ? 'failed' : message.status}
            durationMs={message.durationMs}
            exitCode={message.exitCode}
          />
        ) : null}
        <div className="text-body leading-relaxed text-primary">
          {displayContent ? (
            <MarkdownView content={displayContent} variant="chat" />
          ) : running ? (
            <span className="inline-flex items-center gap-2 text-muted">
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
              {process
                ? t('chat.bubble.generatingPhase', { phase: processPhaseLabel(process.phase, t) })
                : t('chat.bubble.generating')}
            </span>
          ) : (
            <span className="text-muted">{displayError || t('chat.bubble.noOutput')}</span>
          )}
          {displayError && (looksFailed || message.status !== 'ok') && displayContent && (
            <p className="mt-2 text-body text-danger">{displayError}</p>
          )}
        </div>
        {!running && <CopyTextButton text={message.content} />}
      </div>
    </div>
  );
}
