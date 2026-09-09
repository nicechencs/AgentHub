import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import type { AgentProcessView } from '@/lib/chat-process';
import { ChatProcessPanel } from './ChatProcessPanel';

vi.mock('@/components/shared/SourcePreview', () => ({
  SourcePreview: ({ value }: { value: string }) => value,
}));

function view(partial: Partial<AgentProcessView> & Pick<AgentProcessView, 'steps' | 'phase'>): AgentProcessView {
  return {
    turn: 1,
    agent: 'codex',
    stdout: '',
    stderr: '',
    updatedAt: 1,
    ...partial,
  };
}

function renderPanel(process: AgentProcessView, messageStatus?: string) {
  return renderToStaticMarkup(
    createElement(ChatProcessPanel, { view: process, messageStatus, durationMs: 1500 }),
  );
}

describe('ChatProcessPanel human copy', () => {
  it('shows reading/editing/running and folds protocol names into details', () => {
    const html = renderPanel(
      view({
        phase: 'running',
        command: 'codex app-server',
        steps: [
          { type: 'status', phase: 'starting', detail: 'thread.started' },
          {
            type: 'tool',
            name: 'Read',
            status: 'start',
            input: { path: 'README.md' },
          },
        ],
      }),
    );
    expect(html).toContain('正在读取 README.md');
    expect(html).toContain('▸ 正在读取 README.md');
    expect(html).toContain('细节');
    expect(html).toContain('运行详情');
    expect(html).toContain('thread.started');
    expect(html).not.toContain('1 步');
    expect(html).not.toContain('▸ 生成中');
    const humanAt = html.indexOf('正在读取 README.md');
    const detailsAt = html.indexOf('细节');
    const protocolAt = html.indexOf('Read · start');
    expect(humanAt).toBeGreaterThan(-1);
    expect(detailsAt).toBeGreaterThan(humanAt);
    expect(protocolAt).toBeGreaterThan(detailsAt);
  });

  it('summarizes a finished turn with the verbs that happened', () => {
    const html = renderPanel(
      view({
        phase: 'ok',
        steps: [
          { type: 'tool', name: 'Read', status: 'end', input: { path: 'a.ts' } },
          { type: 'tool', name: 'apply_patch', status: 'end', input: { path: 'a.ts' } },
          { type: 'tool', name: 'command_execution', status: 'end', input: { command: 'ls' } },
        ],
      }),
      'ok',
    );
    expect(html).toContain('已完成 · 已读取 · 已修改 · 已执行');
    expect(html).toContain('已读取 a.ts');
    expect(html).toContain('已修改 a.ts');
    expect(html).toContain('已执行 ls');
    const humanRunAt = html.indexOf('已执行 ls');
    const protocolRunAt = html.indexOf('command_execution · end');
    expect(humanRunAt).toBeGreaterThan(-1);
    expect(protocolRunAt).toBeGreaterThan(humanRunAt);
  });
});
