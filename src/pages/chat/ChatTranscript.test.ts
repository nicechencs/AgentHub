import { createElement, createRef, type ReactElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { TooltipProvider } from '@/components/ui/tooltip';
import type { AgentProcessView } from '@/lib/chat-process';
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

function userMessage(content: string, id = 'm-user', turn = 1): ChatMessage {
  return {
    id,
    conversationId: 'c1',
    turn,
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
    expect(html).not.toContain('发送第一条消息');
    expect(html).not.toContain(' · demo');
    expect(html).toContain('了解这个项目');
    expect(html).toContain('检查问题');
    expect(html).toContain('总结当前目录');
    expect(html).toContain('补最小测试');
    expect(html).not.toContain('示例只填入输入框，由你发送');
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
    expect(html).not.toContain(' · demo');
    expect(html).toContain('设置工作目录');
    expect(html).not.toContain('发送第一条消息');
    expect(html).not.toContain('了解这个项目');
    expect(html).not.toContain('示例只填入输入框，由你发送');
  });

  it('puts a clickable thinking bar on the assistant bubble before any body', () => {
    const process: AgentProcessView = {
      turn: 1,
      agent: 'claude',
      phase: 'running',
      stdout: '',
      stderr: '',
      steps: [{ type: 'thinking', text: 'hidden thinking', done: false }],
      updatedAt: 1,
      thinkingStartedAt: Date.now() - 1500,
    };
    const html = renderMarkup(
      createElement(ChatTranscript, {
        active: conversation(),
        turns: [
          {
            turn: 1,
            user: userMessage('hello from chat'),
            agents: [
              {
                id: 'm-agent',
                conversationId: 'c1',
                turn: 1,
                role: 'agent',
                agentId: 'claude',
                content: '',
                status: 'running',
                durationMs: 0,
                createdAt: '2026-08-16T00:00:00.000Z',
              },
            ],
          },
        ],
        processMap: { '1:claude': process },
        listLoading: false,
        messagesLoading: false,
        sending: true,
        retryDisabled: false,
        scrollRef: createRef<HTMLDivElement>(),
        bottomRef: createRef<HTMLDivElement>(),
        onScroll: () => undefined,
        onRetry: () => undefined,
        onOpenProcess: () => undefined,
      }),
    );
    expect(html).toContain('data-help="chat-thinking-bar"');
    expect(html).toContain('思考中');
    expect(html).not.toContain('hidden thinking');
    expect(html).not.toContain('正在想');
  });

  it('does not paint a panel card once a turn exists', () => {
    const html = renderTranscript([
      { turn: 1, user: userMessage('hello from chat'), agents: [] },
    ]);
    expect(html).toContain(`overflow-x-hidden overflow-y-auto ${chatTranscriptSurfaceClass}`);
    expect(html).not.toContain('rounded-composer bg-panel');
    expect(html).not.toContain('rounded-composer bg-canvas');
    expect(html).toContain('hello from chat');
    expect(html).toContain('id="chat-msg-m-user"');
    expect(html).not.toContain('data-testid="chat-outline-rail"');
    expect(html).not.toContain('role="tablist"');
  });

  it('does not mount an outline for an empty transcript or a single user message', () => {
    expect(renderTranscript([])).not.toContain('data-testid="chat-outline-rail"');
    expect(renderTranscript([
      { turn: 1, user: userMessage('hello from chat'), agents: [] },
    ])).not.toContain('chat-outline-rail');
  });

  it('anchors each sent prompt so the outline can jump to it', () => {
    const first = userMessage('first prompt');
    const second = {
      ...userMessage('second prompt'),
      id: 'm-user-2',
      turn: 2,
    };
    const html = renderTranscript([
      { turn: 1, user: first, agents: [] },
      { turn: 2, user: second, agents: [] },
    ]);
    expect(html).toContain('id="chat-msg-m-user"');
    expect(html).toContain('id="chat-msg-m-user-2"');
    expect(html).toContain('first prompt');
    expect(html).toContain('second prompt');
    expect(html).toContain('pointer-events-none');
    // Unmeasured panel width is 0, so the 720px gate still hides the ticks.
    expect(html).not.toContain('data-testid="chat-outline-rail"');
  });

  it('mounts the outline rail when the setting, two prompts, and a 720px panel hold', () => {
    const html = renderMarkup(
      createElement(ChatTranscript, {
        active: conversation(),
        turns: [
          { turn: 1, user: userMessage('first prompt', 'u1', 1), agents: [] },
          { turn: 2, user: userMessage('second prompt', 'u2', 2), agents: [] },
        ],
        processMap: {},
        listLoading: false,
        messagesLoading: false,
        sending: false,
        retryDisabled: false,
        scrollRef: createRef<HTMLDivElement>(),
        bottomRef: createRef<HTMLDivElement>(),
        onScroll: () => undefined,
        onRetry: () => undefined,
        measuredWidth: 720,
        outlineEnabled: true,
      }),
    );
    expect(html).toContain('data-testid="chat-outline-rail"');
    expect(html).toContain('data-testid="chat-outline-tick-u1"');
    expect(html).toContain('data-testid="chat-outline-tick-u2"');
    expect(html).toContain('data-chat-outline-host');
  });

  it('does not mount the rail ticks when only one user message exists', () => {
    const html = renderMarkup(
      createElement(ChatTranscript, {
        active: conversation(),
        turns: [{ turn: 1, user: userMessage('only one', 'u1'), agents: [] }],
        processMap: {},
        listLoading: false,
        messagesLoading: false,
        sending: false,
        retryDisabled: false,
        scrollRef: createRef<HTMLDivElement>(),
        bottomRef: createRef<HTMLDivElement>(),
        onScroll: () => undefined,
        onRetry: () => undefined,
        measuredWidth: 900,
        outlineEnabled: true,
      }),
    );
    expect(html).toContain('data-chat-outline-host');
    expect(html).not.toContain('chat-outline-rail');
  });
});
