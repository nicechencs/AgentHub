import { createElement, type ReactElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { TooltipProvider } from '@/components/ui/tooltip';
import type { AgentProcessView } from '@/lib/chat-process';
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

function renderBubble(message: ChatMessage, process?: AgentProcessView) {
  return renderToStaticMarkup(
    createElement(TooltipProvider, null, createElement(ChatMessageBubble, {
      message,
      process,
      isLastTurn: true,
      multiAgent: false,
      retryDisabled: false,
      onRetry: () => undefined,
    }) as ReactElement),
  );
}

function usageProcess(phase: AgentProcessView['phase']): AgentProcessView {
  return {
    turn: 1,
    agent: 'codex',
    phase,
    stdout: '',
    stderr: '',
    steps: [{ type: 'usage', scope: 'turn', input: 12, output: 3 }],
    updatedAt: 1,
  };
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

  it('shows turn usage as a footer after the reply, not while running', () => {
    const running = renderBubble(agentMessage('第一段正文', 'running'), usageProcess('running'));
    expect(running).not.toContain('输入 12');
    const done = renderBubble(agentMessage('第一段正文', 'ok'), usageProcess('ok'));
    expect(done).toContain('输入 12 · 输出 3');
    expect(done).not.toContain('当前轮');
    expect(done).not.toContain('累计');
  });

  it('hides the bubble retry when the stop banner already has it', () => {
    const html = renderToStaticMarkup(
      createElement(TooltipProvider, null, createElement(ChatMessageBubble, {
        message: agentMessage('', 'cancelled'),
        isLastTurn: true,
        multiAgent: false,
        retryDisabled: false,
        hideRetry: true,
        onRetry: () => undefined,
      })),
    );
    expect(html).not.toContain('重试');
    expect(html).not.toContain('已停止');
  });

  it('opens process details from a one-line chip', () => {
    const process: AgentProcessView = {
      turn: 1,
      agent: 'codex',
      phase: 'running',
      stdout: '',
      stderr: '',
      steps: [{ type: 'tool', name: 'Read', status: 'start', input: { path: 'README.md' } }],
      updatedAt: 1,
    };
    const html = renderToStaticMarkup(
      createElement(TooltipProvider, null, createElement(ChatMessageBubble, {
        message: agentMessage(''),
        process,
        isLastTurn: true,
        multiAgent: false,
        retryDisabled: false,
        onRetry: () => undefined,
        onOpenProcess: () => undefined,
      })),
    );
    expect(html).toContain('data-help="chat-process-chip"');
    expect(html).toContain('▸');
    expect(html).toContain('正在读取 README.md');
    expect(html).not.toContain('用量');
  });

  it('keeps a clickable process row while running even before tools arrive', () => {
    const process: AgentProcessView = {
      turn: 1,
      agent: 'codex',
      phase: 'running',
      stdout: '',
      stderr: '',
      steps: [],
      updatedAt: 1,
    };
    const html = renderToStaticMarkup(
      createElement(TooltipProvider, null, createElement(ChatMessageBubble, {
        message: agentMessage(''),
        process,
        isLastTurn: true,
        multiAgent: false,
        retryDisabled: false,
        onRetry: () => undefined,
        onOpenProcess: () => undefined,
      })),
    );
    expect(html).toContain('data-help="chat-process-chip"');
    expect(html).toContain('▸');
    expect(html).toContain('生成中');
  });
});
