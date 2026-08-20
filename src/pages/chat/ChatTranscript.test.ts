import { createElement, createRef } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import type { ChatMessage, Conversation } from '@/lib/types';
import { chatTranscriptSurfaceClass } from './chat-model';
import { ChatTranscript } from './ChatTranscript';

vi.mock('@/components/shared/MarkdownView', () => ({
  MarkdownView: ({ content }: { content: string }) => content,
}));

function conversation(): Conversation {
  return {
    id: 'c1',
    title: '新对话',
    agentIds: ['claude'],
    cwd: 'D:\\demo',
    allowDangerous: false,
    createdAt: '2026-08-16T00:00:00.000Z',
    updatedAt: '2026-08-16T00:00:00.000Z',
    nativeSessionId: null,
  };
}

function userMessage(content: string): ChatMessage {
  return {
    id: 'm-user',
    conversationId: 'c1',
    turn: 1,
    role: 'user',
    content,
    status: 'ok',
    durationMs: 0,
    createdAt: '2026-08-16T00:00:00.000Z',
  };
}

function renderTranscript(turns: { turn: number; user?: ChatMessage; agents: ChatMessage[] }[]) {
  return renderToStaticMarkup(
    createElement(ChatTranscript, {
      active: conversation(),
      turns,
      processMap: {},
      listLoading: false,
      messagesLoading: false,
      sending: false,
      retryDisabled: false,
      scrollRef: createRef<HTMLDivElement>(),
      bottomRef: createRef<HTMLDivElement>(),
      onScroll: () => undefined,
      onRetry: () => undefined,
    }),
  );
}

describe('ChatTranscript surfaces', () => {
  it('paints canvas when there are no messages, matching composer chrome', () => {
    const html = renderTranscript([]);
    expect(html).toContain(chatTranscriptSurfaceClass(false));
    expect(html).not.toContain(` ${chatTranscriptSurfaceClass(true)}`);
    expect(html).toContain('开始对话');
  });

  it('paints panel once a turn exists, matching the composer input shell', () => {
    const html = renderTranscript([
      { turn: 1, user: userMessage('hello from chat'), agents: [] },
    ]);
    expect(html).toContain(chatTranscriptSurfaceClass(true));
    expect(html).not.toContain(`overflow-y-auto ${chatTranscriptSurfaceClass(false)}`);
    expect(html).toContain('hello from chat');
  });
});
