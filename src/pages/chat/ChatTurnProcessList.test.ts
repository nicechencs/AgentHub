import { createElement, type ReactElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { TooltipProvider } from '@/components/ui/tooltip';
import type { AgentProcessView } from '@/lib/chat-process';
import { ChatTurnProcessList } from './ChatTurnProcessList';
import { processInspectStepKey } from './chat-preview-model';

vi.mock('@/components/shared/LanguageProvider', async () => {
  const { createTranslator } = await import('@/lib/i18n');
  const t = createTranslator('zh');
  return {
    useI18n: () => ({ lang: 'zh', setLanguage: () => undefined, t }),
  };
});

function renderMarkup(node: ReactElement) {
  return renderToStaticMarkup(createElement(TooltipProvider, null, node));
}

function processView(steps: AgentProcessView['steps'], phase: AgentProcessView['phase'] = 'running'): AgentProcessView {
  return {
    turn: 1,
    agent: 'codex',
    phase,
    stdout: '',
    stderr: '',
    steps,
    updatedAt: 1,
    thinkingStartedAt: 1,
    thinkingDurationMs: 3200,
  };
}

describe('ChatTurnProcessList', () => {
  it('lists thinking, read, edit, and execute as separate history rows', () => {
    const html = renderMarkup(
      createElement(ChatTurnProcessList, {
        process: processView([
          { type: 'thinking', text: 'secret plan', done: true },
          { type: 'tool', name: 'Read', status: 'end', input: { path: 'README.md' } },
          { type: 'tool', name: 'Write', status: 'end', input: { path: 'src/a.ts' } },
          { type: 'tool', name: 'Bash', status: 'end', input: { command: 'ls' } },
        ], 'ok'),
        turn: 1,
        agent: 'codex',
        running: false,
        onOpenProcess: () => undefined,
        onSelectEdit: () => undefined,
      }),
    );
    expect(html).toContain('data-help="chat-turn-process"');
    expect(html).toContain('data-help="chat-thinking-bar"');
    expect(html).toContain('思考了 3.2s');
    expect(html).toContain('已读取 README.md');
    expect(html).toContain('已修改 src/a.ts');
    expect(html).toContain('已执行 ls');
    expect(html).toContain('data-help="chat-expand-affordance"');
    expect(html).toContain('可展开');
    expect(html).not.toContain('▸');
    expect(html).not.toContain('▾');
    expect(html).not.toContain('secret plan');
    expect(html).not.toContain('已完成 · 已读取');
  });

  it('opens each file when one edit step changes several paths', () => {
    const html = renderMarkup(
      createElement(ChatTurnProcessList, {
        process: processView([
          {
            type: 'tool',
            name: 'Write',
            status: 'end',
            input: {
              changes: [
                { path: 'src/a.ts', before: 'a', after: 'b' },
                { path: 'src/b.ts', before: 'c', after: 'd' },
              ],
            },
          },
        ], 'ok'),
        turn: 3,
        agent: 'codex',
        running: false,
        selectedEditPath: 'src/b.ts',
        selectedEditTurn: 3,
        onOpenProcess: () => undefined,
        onSelectEdit: () => undefined,
      }),
    );
    expect(html).toContain('已修改 src/a.ts');
    expect(html).toContain('已修改 src/b.ts');
    expect(html.split('aria-current="true"')).toHaveLength(2);
  });

  it('keeps two edits of the same file as separate rows with location', () => {
    const html = renderMarkup(
      createElement(ChatTurnProcessList, {
        process: processView([
          {
            type: 'tool',
            id: 'edit-1',
            name: 'StrReplace',
            status: 'end',
            input: { path: 'src/a.ts', old_string: 'one', new_string: 'two' },
          },
          {
            type: 'tool',
            id: 'edit-2',
            name: 'StrReplace',
            status: 'end',
            input: { path: 'src/a.ts', old_string: 'two', new_string: 'three' },
          },
        ], 'ok'),
        turn: 1,
        agent: 'codex',
        running: false,
        selectedEditPath: 'src/a.ts',
        selectedEditTurn: 1,
        selectedEditStepId: 'edit-1',
        onOpenProcess: () => undefined,
        onSelectEdit: () => undefined,
      }),
    );
    expect(html.match(/已修改 src\/a\.ts/g)?.length).toBe(2);
    expect(html).toContain('+1 −1');
    expect(html.split('aria-current="true"')).toHaveLength(2);
  });

  it('does not mark the same path current on a different turn', () => {
    const html = renderMarkup(
      createElement(ChatTurnProcessList, {
        process: processView([
          { type: 'tool', name: 'Write', status: 'end', input: { path: 'src/a.ts' } },
        ], 'ok'),
        turn: 1,
        agent: 'codex',
        running: false,
        selectedEditPath: 'src/a.ts',
        selectedEditTurn: 4,
        onOpenProcess: () => undefined,
        onSelectEdit: () => undefined,
      }),
    );
    expect(html).toContain('已修改 src/a.ts');
    expect(html).not.toContain('aria-current="true"');
  });

  it('marks only the open step as expanded so another step can take over', () => {
    const html = renderMarkup(
      createElement(ChatTurnProcessList, {
        process: processView([
          { type: 'thinking', text: 'secret plan', done: true },
          { type: 'tool', name: 'Bash', status: 'end', input: { command: 'ls' } },
        ], 'ok'),
        turn: 1,
        agent: 'codex',
        running: false,
        processPaneOpen: true,
        selectedStepKey: processInspectStepKey(0),
        onOpenProcess: () => undefined,
      }),
    );
    const thinking = html.slice(html.indexOf('chat-thinking-bar'), html.indexOf('chat-process-chip'));
    const execute = html.slice(html.indexOf('chat-process-chip'));
    expect(thinking).toContain('aria-expanded="true"');
    expect(thinking).not.toContain('aria-expanded="false"');
    expect(execute).toContain('aria-expanded="false"');
    expect(execute).toContain('已执行 ls');
    expect(execute).not.toContain('aria-expanded="true"');
  });

  it('keeps a generating row when the turn is running with no steps yet', () => {
    const html = renderMarkup(
      createElement(ChatTurnProcessList, {
        process: processView([]),
        turn: 1,
        agent: 'codex',
        running: true,
        onOpenProcess: () => undefined,
      }),
    );
    expect(html).toContain('data-help="chat-process-chip"');
    expect(html).toContain('生成中');
  });

  it('draws nothing when a finished turn only has usage', () => {
    const html = renderMarkup(
      createElement(ChatTurnProcessList, {
        process: processView([{ type: 'usage', scope: 'turn', input: 12, output: 3 }], 'ok'),
        turn: 1,
        agent: 'codex',
        running: false,
        onOpenProcess: () => undefined,
      }),
    );
    expect(html).toBe('');
  });
});
