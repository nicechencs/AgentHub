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

  it('marks the step the transcript switched to without dropping the others', () => {
    const html = renderToStaticMarkup(
      createElement(ChatProcessPanel, {
        view: view({
          phase: 'ok',
          steps: [
            { type: 'thinking', text: '先看工作目录', done: true },
            { type: 'tool', name: 'Bash', status: 'end', input: { command: 'ls' } },
          ],
          thinkingStartedAt: 1,
          thinkingDurationMs: 3200,
        }),
        messageStatus: 'ok',
        activeStepKey: 'step:1',
      }),
    );
    expect(html).toContain('先看工作目录');
    expect(html).toContain('已执行 ls');
    expect(html).toContain('data-process-step-active="true"');
    expect(html.indexOf('先看工作目录')).toBeLessThan(html.indexOf('data-process-step-active="true"'));
    expect(html.indexOf('data-process-step-active="true"')).toBeLessThan(html.indexOf('已执行 ls'));
  });

  it('highlights keywords inside a changed file shown in the detail', () => {
    const html = renderToStaticMarkup(
      createElement(ChatProcessPanel, {
        view: view({
          phase: 'ok',
          steps: [{
            type: 'tool',
            name: 'Write',
            status: 'end',
            input: { path: 'src/app.ts' },
            result: '@@ -1 +1 @@\n-const n = 0;\n+const n = 1;\n',
          }],
        }),
        messageStatus: 'ok',
        activeStepKey: 'step:0',
      }),
    );
    expect(html).toContain('tok-keyword');
    expect(html).toContain('>const<');
    expect(html).toContain('chat-line-number');
    expect(html).toContain('拖动调整代码高度');
    expect(html).toContain('chat-diff-add');
    expect(html).toContain('chat-diff-remove');
    expect(html).toContain('tok-inserted');
    expect(html).toContain('tok-deleted');
    expect(html).toContain('tok-number');
  });

  it('keeps finished thinking expanded in the inspect pane', () => {
    const html = renderPanel(
      view({
        phase: 'ok',
        steps: [{ type: 'thinking', text: '先看工作目录', done: true }],
        thinkingStartedAt: 1,
        thinkingDurationMs: 3200,
      }),
      'ok',
    );
    expect(html).toContain('先看工作目录');
    expect(html).toContain('思考了 3.2s');
    expect(html).toContain('data-help="chat-process-thinking"');
    expect(html).toContain('text-body leading-relaxed text-primary');
    expect(html).toContain('bg-subtle px-3 py-2');
    expect(html).not.toContain('italic text-muted');
    expect(html).toMatch(/<details[^>]*open/);
  });

  it('shows a thinking fold without a tool row when no tools ran', () => {
    const html = renderPanel(
      view({
        phase: 'running',
        steps: [{ type: 'thinking', text: 'only thinking', done: false }],
        thinkingStartedAt: Date.now() - 800,
      }),
    );
    expect(html).toContain('data-help="chat-process-thinking"');
    expect(html).toContain('only thinking');
    expect(html).not.toContain('data-help="chat-process-tool"');
  });

  it('keeps an error on the tools timeline, not inside the thinking fold', () => {
    const html = renderPanel(
      view({
        phase: 'failed',
        steps: [
          { type: 'thinking', text: 'tried', done: true },
          { type: 'error', message: 'disk full' },
        ],
        thinkingStartedAt: 1,
        thinkingDurationMs: 400,
      }),
      'failed',
    );
    expect(html).toContain('data-help="chat-process-thinking"');
    expect(html).toContain('tried');
    expect(html).toContain('disk full');
    expect(html).toContain('text-danger');
    const thinkingAt = html.indexOf('data-help="chat-process-thinking"');
    const errorAt = html.indexOf('disk full');
    expect(errorAt).toBeGreaterThan(thinkingAt);
  });

  it('keeps thinking as a fold separate from tool rows and pins live text', () => {
    const html = renderPanel(
      view({
        phase: 'running',
        steps: [
          { type: 'thinking', text: '先看目录再改', done: false },
          { type: 'tool', name: 'Read', status: 'start', input: { path: 'README.md' } },
        ],
        thinkingStartedAt: Date.now() - 3200,
      }),
    );
    expect(html).toContain('data-help="chat-process-thinking"');
    expect(html).toContain('data-help="chat-process-tool"');
    expect(html).toContain('思考中');
    expect(html).toContain('正在读取 README.md');
    expect(html).toContain('[overflow-anchor:none]');
    expect(html).toContain('拖动调整思考高度');
    expect(html).toContain('拖动调整 JSON 高度');
    const thinkingAt = html.indexOf('data-help="chat-process-thinking"');
    const toolAt = html.indexOf('data-help="chat-process-tool"');
    expect(thinkingAt).toBeGreaterThan(-1);
    expect(toolAt).toBeGreaterThan(thinkingAt);
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

  it('does not draw an empty JSON preview in tool details', () => {
    const html = renderPanel(
      view({
        phase: 'ok',
        steps: [
          {
            type: 'tool',
            name: 'read',
            status: 'end',
            input: {},
            result:
              '{ "__tool_use_purpose": "Inspect ManifestSyncer symbols." }\nThe tool input does not match the tool schema: missing field `operation`',
          },
        ],
      }),
      'ok',
    );
    expect(html).toContain('细节');
    expect(html).toContain('read · end');
    expect(html).toContain('Inspect ManifestSyncer symbols.');
    expect(html).not.toContain('{}');
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

  it('offers copy and a remembered height drag on command and process log', () => {
    const html = renderPanel(
      view({
        phase: 'running',
        command: 'dsh --profile headless',
        stderr: 'line 26: rebase onto latest main',
        steps: [],
      }),
    );
    expect(html).toContain('aria-label="复制"');
    expect(html).toContain('拖动调整命令高度');
    expect(html).toContain('拖动调整过程日志高度');
    expect(html).toContain('cursor-row-resize');
    expect(html).toContain('dsh');
    expect(html).toContain('--profile');
    expect(html).toContain('headless');
    expect(html).toContain('tok-propertyName');
    expect(html).not.toContain('max-h-24');
    expect(html).not.toContain('max-h-36');
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
