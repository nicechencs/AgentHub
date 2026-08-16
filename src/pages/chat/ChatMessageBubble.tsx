import { Copy, Loader2 } from 'lucide-react';
import { AgentLogo } from '@/components/shared/AgentLogo';
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
        className="group relative max-w-[85%] rounded-composer bg-subtle px-4 py-2 text-sm text-primary"
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
  const agent = message.agentId ?? 'claude';
  const statusText = messageStatusLabel(message.status, process);
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
        <div className="mb-1 flex flex-wrap items-center gap-2 text-xs text-muted">
          <span className="font-medium text-secondary">{agentDisplayName(agent)}</span>
          {statusText && <span>{statusText}</span>}
          {message.durationMs > 0 && <span>{formatDurationMs(message.durationMs)}</span>}
          {showRetry && (
            <Hint
              label={
                multiAgent ? '将把这条提示重新发给会话中的全部 Agent' : undefined
              }
            >
              <button
                type="button"
                disabled={retryDisabled}
                onClick={onRetry}
                className="rounded-btn px-1.5 py-0.5 text-xs text-secondary hover:bg-hover hover:text-primary disabled:opacity-50"
              >
                重试
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
        <div className="text-sm leading-relaxed text-primary">
          {message.content ? (
            <MarkdownView content={message.content} variant="chat" />
          ) : running ? (
            <span className="inline-flex items-center gap-2 text-muted">
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
              {process ? `${processPhaseLabel(process.phase)}…` : '生成中…'}
            </span>
          ) : (
            <span className="text-muted">{message.error || '（无输出）'}</span>
          )}
          {message.error && message.status !== 'ok' && message.content && (
            <p className="mt-2 text-sm text-danger">{message.error}</p>
          )}
        </div>
        {!running && <CopyButton text={message.content} />}
      </div>
    </div>
  );
}

function CopyButton({ text }: { text: string }) {
  const { toast } = useToast();
  if (!text) return null;
  return (
    <button
      type="button"
      aria-label="复制"
      className={cn(
        'absolute bottom-1 right-1 rounded-btn p-1 text-muted',
        'opacity-0 transition-opacity hover:bg-panel hover:text-primary group-hover:opacity-100',
      )}
      onClick={() => {
        void navigator.clipboard.writeText(text).then(
          () => toast({ title: '已复制', variant: 'success' }),
          () => toast({ title: '复制失败', variant: 'danger' }),
        );
      }}
    >
      <Copy className="h-3.5 w-3.5" />
    </button>
  );
}
