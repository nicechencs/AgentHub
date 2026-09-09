import { createElement, createRef, type ReactElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { TooltipProvider } from '@/components/ui/tooltip';
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

function renderMarkup(node: ReactElement) {
  return renderToStaticMarkup(createElement(TooltipProvider, null, node));
}

function renderTranscript(turns: { turn: number; user?: ChatMessage; agents: ChatMessage[] }[]) {
  return renderMarkup(
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
    expect(html).toContain(`overflow-x-hidden overflow-y-auto ${chatTranscriptSurfaceClass}`);
    expect(html).not.toContain('rounded-composer bg-panel');
    expect(html).not.toContain('rounded-composer bg-canvas');
    expect(html).toContain('text-display');
    expect(html).toContain('开始对话');
    expect(html).toContain('发送第一条消息');
    expect(html).toContain(' · demo');
    expect(html).toContain('了解这个项目');
    expect(html).toContain('检查问题');
    expect(html).toContain('总结当前目录');
    expect(html).toContain('补最小测试');
    expect(html).toContain('示例只填入输入框，由你发送');
    expect(html).not.toContain('请帮我了解这个项目的结构和主要功能。');
  });

  it('replaces starters with the first send blocker as the primary action', () => {
    const html = renderMarkup(
      createElement(ChatTranscript, {
        active: conversation(),
        turns: [],
        processMap: {},
        listLoading: false,
        messagesLoading: false,
        sending: false,
        retryDisabled: false,
        scrollRef: createRef<HTMLDivElement>(),
        bottomRef: createRef<HTMLDivElement>(),
        onScroll: () => undefined,
        onRetry: () => undefined,
        firstBlocker: { kind: 'noCwd' },
      }),
    );
    expect(html).toContain('开始对话');
    expect(html).toContain(' · demo');
    expect(html).toContain('设置工作目录');
    expect(html).not.toContain('发送第一条消息');
    expect(html).not.toContain('了解这个项目');
    expect(html).not.toContain('示例只填入输入框，由你发送');
  });

  it('does not paint a panel card once a turn exists', () => {
    const html = renderTranscript([
      { turn: 1, user: userMessage('hello from chat'), agents: [] },
    ]);
    expect(html).toContain(`overflow-x-hidden overflow-y-auto ${chatTranscriptSurfaceClass}`);
    expect(html).not.toContain('rounded-composer bg-panel');
    expect(html).not.toContain('rounded-composer bg-canvas');
    expect(html).toContain('hello from chat');
  });
});
