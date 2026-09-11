import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import type { AgentProcessView } from '@/lib/chat-process';
import { ChatProcessPanel } from './ChatProcessPanel';

vi.mock('@/components/shared/SourcePreview', () => ({
  SourcePreview: ({ value, showCopy }: { value: string; showCopy?: boolean }) =>
    `${showCopy ? '复制' : ''}${value}`,
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
    createElement(ChatProcessPanel, { view: process, messageStatus }),
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
    expect(html).not.toContain('▸ 正在读取 README.md');
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
    expect(html).toContain('已读取 a.ts');
    expect(html).toContain('已修改 a.ts');
    expect(html).toContain('已执行 ls');
    const humanRunAt = html.indexOf('已执行 ls');
    const protocolRunAt = html.indexOf('command_execution · end');
    expect(humanRunAt).toBeGreaterThan(-1);
    expect(protocolRunAt).toBeGreaterThan(humanRunAt);
  });

  it('folds command output chunks into 已执行 instead of listing each 细节', () => {
    const html = renderPanel(
      view({
        phase: 'ok',
        steps: [
          { type: 'raw', text: 'docs\n', note: 'command output' },
          { type: 'raw', text: 'src\n', note: 'command output' },
          { type: 'thinking', text: '先看目录', done: true },
        ],
      }),
      'ok',
    );
    expect(html).toContain('已执行');
    expect(html).toContain('先看目录');
    expect(html).not.toContain('command output');
    expect(html).toContain('docs');
  });

  it('keeps finished thinking expanded in the inspect pane', () => {
    const html = renderPanel(
      view({
        phase: 'ok',
        steps: [{ type: 'thinking', text: '先看工作目录', done: true }],
      }),
      'ok',
    );
    expect(html).toContain('先看工作目录');
    expect(html).toMatch(/<details[^>]*open/);
  });

  it('offers one-click copy on JSON in tool details', () => {
    const html = renderPanel(
      view({
        phase: 'ok',
        steps: [
          {
            type: 'tool',
            name: 'Read',
            status: 'end',
            input: { mode: 'Directory', path: 'D:\\foo', depth: 2 },
          },
        ],
      }),
      'ok',
    );
    expect(html).toContain('细节');
    expect(html).toContain('复制');
    expect(html).toContain('Directory');
  });

  it('labels command stderr as a process log, not an error', () => {
    const html = renderPanel(
      view({
        phase: 'running',
        command: 'dsh --profile headless',
        stderr: 'line 26: rebase onto latest main',
        steps: [],
      }),
    );
    expect(html).toContain('过程日志');
    expect(html).not.toContain('错误输出');
    expect(html).toContain('line 26: rebase onto latest main');
    expect(html).not.toMatch(/text-danger/);
  });

  it('opens run details when the timeline is empty so the pane is not blank', () => {
    const html = renderPanel(
      view({
        phase: 'ok',
        command: 'codex app-server',
        steps: [{ type: 'status', phase: 'starting', detail: 'thread.started' }],
      }),
      'ok',
    );
    expect(html).toContain('运行详情');
    expect(html).toMatch(/<details[^>]*open/);
    expect(html).toContain('codex app-server');
  });
});
