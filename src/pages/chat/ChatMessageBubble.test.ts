import { createElement, type ReactElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { TooltipProvider } from '@/components/ui/tooltip';
import type { ChatMessage } from '@/lib/types';
import { ChatMessageBubble } from './ChatMessageBubble';

vi.mock('@/components/shared/MarkdownView', () => ({
  MarkdownView: ({ content }: { content: string }) => content,
}));

function agentMessage(content: string, status: ChatMessage['status'] = 'running'): ChatMessage {
  return {
    id: 'm-agent',
    conversationId: 'c1',
    turn: 1,
    role: 'agent',
    agentId: 'codex',
    content,
    status,
    durationMs: 0,
    createdAt: '2026-09-09T00:00:00.000Z',
  };
}

function renderBubble(message: ChatMessage) {
  return renderToStaticMarkup(
    createElement(TooltipProvider, null, createElement(ChatMessageBubble, {
      message,
      isLastTurn: true,
      multiAgent: false,
      retryDisabled: false,
      onRetry: () => undefined,
    }) as ReactElement),
  );
}

describe('ChatMessageBubble streaming feel', () => {
  it('shows 正在想 before the first character', () => {
    const html = renderBubble(agentMessage(''));
    expect(html).toContain('正在想');
    expect(html).toContain('data-chat-stream-activity="thinking"');
    expect(html).not.toContain('chat-stream-caret');
    expect(html).not.toContain('chat-stream-in');
  });

  it('keeps arrived text on screen and marks writing without replaying a whole-block fade', () => {
    const html = renderBubble(agentMessage('第一段正文'));
    expect(html).toContain('第一段正文');
    expect(html).toContain('正在写');
    expect(html).toContain('data-chat-stream-activity="writing"');
    expect(html).toContain('chat-stream-caret');
    expect(html).not.toContain('chat-stream-in');
  });
});
