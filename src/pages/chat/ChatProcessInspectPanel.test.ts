import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { TooltipProvider } from '@/components/ui/tooltip';
import type { AgentProcessView } from '@/lib/chat-process';
import { ChatProcessInspectPanel } from './ChatProcessInspectPanel';

vi.mock('@/components/shared/SourcePreview', () => ({
  SourcePreview: ({ value }: { value: string }) => value,
}));

function view(): AgentProcessView {
  return {
    turn: 1,
    agent: 'codex',
    phase: 'ok',
    stdout: '',
    stderr: '',
    updatedAt: 1,
    steps: [
      { type: 'tool', name: 'Read', status: 'end', input: { path: 'a.ts' } },
    ],
  };
}

describe('ChatProcessInspectPanel', () => {
  it('uses the process headline as the pane title', () => {
    const html = renderToStaticMarkup(
      createElement(
        TooltipProvider,
        null,
        createElement(ChatProcessInspectPanel, {
          view: view(),
          messageStatus: 'ok',
          open: true,
          onClose: () => undefined,
        }),
      ),
    );
    expect(html).toContain('data-help="chat-process-inspect"');
    expect(html).toContain('已完成 · 已读取');
    expect(html).toContain('已读取 a.ts');
  });
});
