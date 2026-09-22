import { AgentLogo } from '@/components/shared/AgentLogo';
import { AgentThinking } from '@/components/shared/AgentThinking';
import { CopyTextButton } from '@/components/shared/CopyTextButton';
import { useI18n } from '@/components/shared/LanguageProvider';
import { MarkdownView, type MarkdownOpenLocalOptions } from '@/components/shared/MarkdownView';
import { agentDisplayName } from '@/config/agents';
import {
  formatTurnUsageFooter,
  transcriptTimelineSteps,
} from '@/lib/chat-process';
import type { AgentProcessView } from '@/lib/chat-process';
import type { AgentKey, ChatMessage } from '@/lib/types';
import {
  formatChatDisplayContent,
  formatDurationMs,
  localizeChatFailure,
  looksLikeChatProtocolDump,
  sanitizeCliChatText,
} from './chat-format';
import type { TurnEditFile } from './chat-edit-preview';
import { messageStatusLabel } from './chat-model';
import { ChatTurnProcessList } from './ChatTurnProcessList';
import { streamingActivity, streamingPlaceholderKey } from './chat-streaming';

export function ChatMessageBubble({
  message,
  process,
  localBasePath,
  onOpenLocal,
  onOpenProcess,
  onCloseProcess,
  processPaneOpen = false,
  selectedEditPath = '',
  selectedEditTurn,
  onSelectEdit,
}: {
  message: ChatMessage;
  process?: AgentProcessView;
  localBasePath?: string;
  onOpenLocal?: (path: string, options?: MarkdownOpenLocalOptions) => boolean;
  onOpenProcess?: (turn: number, agent: AgentKey) => void;
  onCloseProcess?: () => void;
  processPaneOpen?: boolean;
  selectedEditPath?: string;
  selectedEditTurn?: number;
  onSelectEdit?: (file: TurnEditFile, turn: number) => void;
}) {
  if (message.role === 'user') {
    return (
      <UserBubble message={message} localBasePath={localBasePath} onOpenLocal={onOpenLocal} />
    );
  }
  return (
    <AgentBubble
      message={message}
      process={process}
      localBasePath={localBasePath}
      onOpenLocal={onOpenLocal}
      onOpenProcess={onOpenProcess}
      onCloseProcess={onCloseProcess}
      processPaneOpen={processPaneOpen}
      selectedEditPath={selectedEditPath}
      selectedEditTurn={selectedEditTurn}
      onSelectEdit={onSelectEdit}
    />
  );
}

function UserBubble({
  message,
  localBasePath,
  onOpenLocal,
}: {
  message: ChatMessage;
  localBasePath?: string;
  onOpenLocal?: (path: string, options?: MarkdownOpenLocalOptions) => boolean;
}) {
  return (
    <div className="flex justify-end">
      <div
        id={`chat-msg-${message.id}`}
        className="group relative max-w-[85%] rounded-composer bg-subtle px-4 py-2 text-body leading-relaxed text-primary"
      >
        <MarkdownView
          content={message.content}
          variant="chat"
          localBasePath={localBasePath}
          onOpenLocal={onOpenLocal}
        />
        <CopyTextButton text={message.content} />
      </div>
    </div>
  );
}

function AgentBubble({
  message,
  process,
  localBasePath,
  onOpenLocal,
  onOpenProcess,
  onCloseProcess,
  processPaneOpen,
  selectedEditPath,
  selectedEditTurn,
  onSelectEdit,
}: {
  message: ChatMessage;
  process?: AgentProcessView;
  localBasePath?: string;
  onOpenLocal?: (path: string, options?: MarkdownOpenLocalOptions) => boolean;
  onOpenProcess?: (turn: number, agent: AgentKey) => void;
  onCloseProcess?: () => void;
  processPaneOpen: boolean;
  selectedEditPath: string;
  selectedEditTurn?: number;
  onSelectEdit?: (file: TurnEditFile, turn: number) => void;
}) {
  const { t } = useI18n();
  const agent = message.agentId ?? 'claude';
  const protocolDump = looksLikeChatProtocolDump(message.content);
  const localized =
    message.content && !protocolDump ? localizeChatFailure(message.content, t) : '';
  const displayContent = localized
    ? formatChatDisplayContent(sanitizeCliChatText(localized))
    : '';
  const cancelledPlaceholder =
    message.status === 'cancelled' && ((message.error ?? '').toLowerCase() === 'cancelled');
  const displayError =
    cancelledPlaceholder || protocolDump
      ? ''
      : message.error
        ? localizeChatFailure(message.error, t)
        : '';
  const looksFailed =
    message.status === 'failed' ||
    message.status === 'cancelled' ||
    message.status === 'timeout' ||
    (message.status === 'ok' && localized !== message.content);
  const running = message.status === 'running';
  const hasContent = Boolean(displayContent);
  const resolvedStatus = looksFailed && message.status === 'ok' ? 'failed' : message.status;
  const hasTimeline = transcriptTimelineSteps(process?.steps).length > 0;
  const showProcessList = Boolean(onOpenProcess) && (
    running || hasTimeline
  );
  const terminalFailure =
    resolvedStatus === 'failed' ||
    resolvedStatus === 'cancelled' ||
    resolvedStatus === 'timeout';
  const statusText = showProcessList && !terminalFailure
    ? null
    : messageStatusLabel(t, resolvedStatus, process, hasContent);
  const activity = running ? streamingActivity(process, hasContent) : null;
  const usageText = formatTurnUsageFooter(process?.steps, running, t);

  return (
    <div id={`chat-msg-${message.id}`} className="group flex min-w-0 gap-3">
      <AgentLogo agentId={agent} size="md" />
      <div className="relative min-w-0 flex-1 pt-0.5">
        <div className="mb-1 flex flex-wrap items-center gap-2 text-meta text-muted">
          <span className="font-medium text-secondary">{agentDisplayName(agent)}</span>
          {statusText ? <span>{statusText}</span> : null}
          {message.durationMs > 0 && <span>{formatDurationMs(message.durationMs)}</span>}
        </div>
        {showProcessList ? (
          <ChatTurnProcessList
            process={process}
            turn={message.turn}
            agent={agent}
            running={running}
            processPaneOpen={processPaneOpen}
            selectedEditPath={selectedEditPath}
            selectedEditTurn={selectedEditTurn}
            onOpenProcess={onOpenProcess}
            onCloseProcess={onCloseProcess}
            onSelectEdit={onSelectEdit}
          />
        ) : null}
        <div
          className="min-w-0 overflow-hidden text-body leading-relaxed text-primary"
          data-chat-stream-activity={activity ?? undefined}
        >
          {displayContent ? (
            <div>
              <MarkdownView
                content={displayContent}
                variant="chat"
                localBasePath={localBasePath}
                onOpenLocal={onOpenLocal}
              />
              {running ? <span className="chat-stream-caret" aria-hidden /> : null}
            </div>
          ) : running && !hasTimeline ? (
            <AgentThinking label={t(streamingPlaceholderKey(process))} />
          ) : !running ? (
            <span className="text-muted">
              {displayError
                || (cancelledPlaceholder ? t('chat.turnOutcome.cancelledHint') : t('chat.bubble.noOutput'))}
            </span>
          ) : null}
          {displayError && (looksFailed || message.status !== 'ok') && displayContent && (
            <p className="mt-2 text-body leading-relaxed text-danger">{displayError}</p>
          )}
        </div>
        {usageText ? (
          <p className="mt-1 text-meta text-muted">{usageText}</p>
        ) : null}
        {!running && (
          <CopyTextButton text={protocolDump ? '' : sanitizeCliChatText(message.content)} />
        )}
      </div>
    </div>
  );
}
