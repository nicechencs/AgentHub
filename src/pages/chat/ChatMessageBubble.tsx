import { useEffect, useState } from 'react';
import { AgentLogo } from '@/components/shared/AgentLogo';
import { AgentThinking } from '@/components/shared/AgentThinking';
import { CopyTextButton } from '@/components/shared/CopyTextButton';
import { useI18n } from '@/components/shared/LanguageProvider';
import { MarkdownView, type MarkdownOpenLocalOptions } from '@/components/shared/MarkdownView';
import { agentDisplayName } from '@/config/agents';
import {
  formatProcessHeadline,
  formatTurnUsageFooter,
  hasInspectableProcess,
  latestThinkingStep,
  phaseFromMessageStatus,
  showBubbleThinkingBar,
  thinkingElapsedMs,
  timelineHasToolRow,
} from '@/lib/chat-process';
import type { AgentProcessView } from '@/lib/chat-process';
import type { AgentKey, ChatMessage } from '@/lib/types';
import {
  formatChatDisplayContent,
  formatDurationMs,
  localizeChatFailure,
  looksLikeChatProtocolDump,
  sanitizeCliChatText,
  thinkingChromeLabel,
} from './chat-format';
import { messageStatusLabel } from './chat-model';
import { streamingActivity, streamingPlaceholderKey } from './chat-streaming';

export function ChatMessageBubble({
  message,
  process,
  localBasePath,
  onOpenLocal,
  onOpenProcess,
  onCloseProcess,
  processPaneOpen = false,
}: {
  message: ChatMessage;
  process?: AgentProcessView;
  localBasePath?: string;
  onOpenLocal?: (path: string, options?: MarkdownOpenLocalOptions) => boolean;
  onOpenProcess?: (turn: number, agent: AgentKey) => void;
  onCloseProcess?: () => void;
  processPaneOpen?: boolean;
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
}: {
  message: ChatMessage;
  process?: AgentProcessView;
  localBasePath?: string;
  onOpenLocal?: (path: string, options?: MarkdownOpenLocalOptions) => boolean;
  onOpenProcess?: (turn: number, agent: AgentKey) => void;
  onCloseProcess?: () => void;
  processPaneOpen: boolean;
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
  const effectivePhase = process
    ? resolvedStatus && resolvedStatus !== 'running'
      ? phaseFromMessageStatus(resolvedStatus)
      : process.phase
    : running
      ? 'running'
      : null;
  const thinking = latestThinkingStep(process?.steps);
  const showThinkingBar = showBubbleThinkingBar(process?.steps, hasContent);
  const showProcessChip = Boolean(onOpenProcess) && (
    running || Boolean(process && hasInspectableProcess(process))
  ) && (!showThinkingBar || timelineHasToolRow(process?.steps));
  const processHeadline = showProcessChip
    ? process && effectivePhase
      ? formatProcessHeadline(process.steps, effectivePhase, t)
      : messageStatusLabel(t, resolvedStatus, process, hasContent) ?? t('chat.process.summaryGenerating')
    : '';
  const statusText = showProcessChip || showThinkingBar
    ? null
    : messageStatusLabel(t, resolvedStatus, process, hasContent);
  const activity = running ? streamingActivity(process, hasContent) : null;
  const usageText = formatTurnUsageFooter(process?.steps, running, t);
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!showThinkingBar || thinking?.done) return;
    const id = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(id);
  }, [showThinkingBar, thinking?.done]);
  const thinkingLabel = thinking
    ? thinkingChromeLabel(Boolean(thinking.done), thinkingElapsedMs(process, now), t)
    : '';

  return (
    <div id={`chat-msg-${message.id}`} className="group flex min-w-0 gap-3">
      <AgentLogo agentId={agent} size="md" />
      <div className="relative min-w-0 flex-1 pt-0.5">
        <div className="mb-1 flex flex-wrap items-center gap-2 text-meta text-muted">
          <span className="font-medium text-secondary">{agentDisplayName(agent)}</span>
          {statusText ? <span>{statusText}</span> : null}
          {message.durationMs > 0 && <span>{formatDurationMs(message.durationMs)}</span>}
        </div>
        {showThinkingBar && thinkingLabel ? (
          <button
            type="button"
            className="mb-1 inline-flex max-w-full items-center gap-1 rounded-btn px-1 py-0.5 text-left text-meta text-secondary hover:bg-hover hover:text-primary"
            data-help="chat-thinking-bar"
            aria-expanded={processPaneOpen}
            disabled={!onOpenProcess}
            onClick={() => {
              if (!onOpenProcess) return;
              if (processPaneOpen) onCloseProcess?.();
              else onOpenProcess(message.turn, agent);
            }}
          >
            <span className="shrink-0" aria-hidden>
              {processPaneOpen ? '▾' : '▸'}
            </span>
            {thinking?.done ? (
              <span className="min-w-0 truncate">{thinkingLabel}</span>
            ) : (
              <AgentThinking label={thinkingLabel} showTimer={false} />
            )}
          </button>
        ) : null}
        {showProcessChip && processHeadline && onOpenProcess ? (
          <button
            type="button"
            className="mb-1 inline-flex max-w-full items-center gap-1 rounded-btn px-1 py-0.5 text-left text-meta text-secondary hover:bg-hover hover:text-primary"
            data-help="chat-process-chip"
            aria-expanded={processPaneOpen}
            onClick={() => {
              if (processPaneOpen) onCloseProcess?.();
              else onOpenProcess(message.turn, agent);
            }}
          >
            <span className="shrink-0" aria-hidden>
              {processPaneOpen ? '▾' : '▸'}
            </span>
            <span className="min-w-0 truncate">{processHeadline}</span>
          </button>
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
          ) : running && !showThinkingBar ? (
            <AgentThinking label={t(streamingPlaceholderKey(process))} />
          ) : !running ? (
            <span className="text-muted">{displayError || t('chat.bubble.noOutput')}</span>
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
