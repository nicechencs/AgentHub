import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { TooltipProvider } from '@/components/ui/tooltip';
import type { ProcessMap } from '@/lib/chat-process';
import type { ProcessStep } from '@/lib/types';
import { ChatEditPreviewPanel, ChatTurnEditList } from './ChatEditPreviewPanel';
import {
  extractEditFilesFromSteps,
  extractTurnEdits,
  formatSimpleDiff,
  latestProcessTurn,
  sameEditPath,
  turnEditDiffText,
  turnEditHasInlineDiff,
} from './chat-edit-preview';

vi.mock('@/components/shared/SourcePreview', () => ({
  SourcePreview: ({ value, fileName }: { value: string; fileName: string }) =>
    `PREVIEW:${fileName}:${value}`,
}));

function tool(
  name: string,
  status: string,
  input?: unknown,
  result?: string | null,
): ProcessStep {
  return { type: 'tool', name, status, input, result };
}

function mapWith(steps: ProcessStep[], turn = 1, agent: 'codex' | 'grok' = 'codex'): ProcessMap {
  return {
    [`${turn}:${agent}`]: {
      turn,
      agent,
      phase: 'running',
      stdout: '',
      stderr: '',
      updatedAt: turn,
      steps,
    },
  };
}

describe('extractEditFilesFromSteps', () => {
  it('keeps live Write paths as 正在修改 and completed as 已修改', () => {
    expect(
      extractEditFilesFromSteps([
        tool('Write', 'start', { path: 'src/a.ts' }),
      ]),
    ).toEqual([{ path: 'src/a.ts', status: 'live' }]);
    expect(
      extractEditFilesFromSteps([
        tool('Write', 'end', { path: 'src/a.ts' }),
      ]),
    ).toEqual([{ path: 'src/a.ts', status: 'done' }]);
  });

  it('ignores read / execute tools and failed edits', () => {
    expect(
      extractEditFilesFromSteps([
        tool('Read', 'end', { path: 'README.md' }),
        tool('Bash', 'end', { command: 'ls' }),
        tool('Write', 'error', { path: 'src/a.ts' }),
      ]),
    ).toEqual([]);
  });

  it('reads target_file, file_path, and file:// URIs', () => {
    expect(
      extractEditFilesFromSteps([
        tool('Edit', 'end', { target_file: '/workspace/src/app.ts' }),
        tool('StrReplace', 'end', { file_path: 'lib\\b.ts' }),
        tool('Delete', 'end', { uri: 'file://localhost/workspace/README.md' }),
      ]),
    ).toEqual([
      { path: '/workspace/src/app.ts', status: 'done' },
      { path: 'lib\\b.ts', status: 'done' },
      { path: '/workspace/README.md', status: 'done' },
    ]);
  });

  it('dedupes the same path and keeps the later status', () => {
    expect(
      extractEditFilesFromSteps([
        tool('Write', 'start', { path: 'src/a.ts' }),
        tool('StrReplace', 'end', { path: 'src\\a.ts', old_string: 'a', new_string: 'b' }),
      ]),
    ).toEqual([
      {
        path: 'src\\a.ts',
        status: 'done',
        before: 'a',
        after: 'b',
      },
    ]);
  });

  it('copies old/new text from StrReplace input', () => {
    const files = extractEditFilesFromSteps([
      tool('StrReplace', 'end', {
        path: 'src/app.ts',
        old_string: "const name = 'old';",
        new_string: "const name = 'new';",
      }),
    ]);
    expect(files[0]?.before).toBe("const name = 'old';");
    expect(files[0]?.after).toBe("const name = 'new';");
    expect(turnEditHasInlineDiff(files[0]!)).toBe(true);
  });

  it('copies nested changes[] and fileChanges map', () => {
    expect(
      extractEditFilesFromSteps([
        tool('apply_patch', 'end', {
          changes: [
            { path: 'src/app.ts', before: 'old', after: 'new' },
            { path: 'src/b.ts' },
          ],
        }),
      ]),
    ).toEqual([
      { path: 'src/app.ts', status: 'done', before: 'old', after: 'new' },
      { path: 'src/b.ts', status: 'done' },
    ]);

    const written = extractEditFilesFromSteps([
      tool('apply_patch', 'end', {
        fileChanges: {
          '/tmp/example.txt': { type: 'add', content: 'ok' },
        },
      }),
    ]);
    expect(written).toEqual([
      { path: '/tmp/example.txt', status: 'done', after: 'ok' },
    ]);
    expect(turnEditHasInlineDiff(written[0]!)).toBe(false);
  });

  it('reads grok operation.diff when it looks like a unified diff', () => {
    const files = extractEditFilesFromSteps([
      tool('edit', 'end', {
        toolCall: {
          kind: 'edit',
          rawInput: {
            operation: {
              type: 'update_file',
              path: 'README.md',
              diff: '@@ -1,2 +1,3 @@\n hello\n+world\n',
            },
          },
        },
      }),
    ]);
    expect(files[0]?.path).toBe('README.md');
    expect(files[0]?.diff).toContain('@@ -1,2 +1,3 @@');
    expect(turnEditHasInlineDiff(files[0]!)).toBe(true);
  });

  it('does not treat a protocol snippet without old/new as an inline diff', () => {
    const files = extractEditFilesFromSteps([
      tool('file_change', 'end', {
        item: {
          changes: [
            {
              path: '/workspace/probe.txt',
              kind: { type: 'add' },
              diff: 'FILECHANGE_OK\n',
            },
          ],
        },
      }),
    ]);
    expect(files[0]?.path).toBe('/workspace/probe.txt');
    expect(turnEditHasInlineDiff(files[0]!)).toBe(false);
  });

  it('parses JSON tool results and a path on the tool name', () => {
    expect(
      extractEditFilesFromSteps([
        tool(
          'Write',
          'end',
          { path: 'notes.md' },
          JSON.stringify({ old_string: 'a', new_string: 'b' }),
        ),
      ]),
    ).toEqual([
      { path: 'notes.md', status: 'done', before: 'a', after: 'b' },
    ]);
    expect(
      extractEditFilesFromSteps([tool('Write src/named.ts', 'end')]),
    ).toEqual([{ path: 'src/named.ts', status: 'done' }]);
  });

  it('reads locations[], files[], and file:// without localhost', () => {
    expect(
      extractEditFilesFromSteps([
        tool('apply_patch', 'end', {
          locations: [{ path: 'src/c.ts', old_text: 'c1', new_text: 'c2' }],
        }),
      ]),
    ).toEqual([{ path: 'src/c.ts', status: 'done', before: 'c1', after: 'c2' }]);
    expect(
      extractEditFilesFromSteps([
        tool('Write', 'end', { files: ['src/a.ts', { filePath: 'src/b.ts' }] }),
      ]),
    ).toEqual([
      { path: 'src/a.ts', status: 'done' },
      { path: 'src/b.ts', status: 'done' },
    ]);
    expect(
      extractEditFilesFromSteps([
        tool('Edit', 'end', { uri: 'file:///workspace/notes.md' }),
      ]),
    ).toEqual([{ path: '/workspace/notes.md', status: 'done' }]);
  });

  it('ignores a bare string, a space-only name, and invalid JSON results', () => {
    expect(extractEditFilesFromSteps([tool('Write', 'end', 'hello world')])).toEqual([]);
    expect(extractEditFilesFromSteps([tool('Write readme', 'end')])).toEqual([]);
    expect(
      extractEditFilesFromSteps([
        tool('Write', 'end', { path: 'notes.md' }, '{not-json'),
      ]),
    ).toEqual([{ path: 'notes.md', status: 'done' }]);
  });

  it('stops walking nested item wrappers after three levels', () => {
    expect(
      extractEditFilesFromSteps([
        tool('Write', 'end', {
          item: { item: { item: { item: { path: 'too-deep.ts' } } } },
        }),
      ]),
    ).toEqual([]);
    expect(
      extractEditFilesFromSteps([
        tool('Write', 'end', {
          item: { item: { path: 'ok.ts' } },
        }),
      ]),
    ).toEqual([{ path: 'ok.ts', status: 'done' }]);
  });
});

describe('extractTurnEdits', () => {
  it('uses the latest turn and merges agents on that turn', () => {
    const processMap: ProcessMap = {
      ...mapWith([tool('Write', 'end', { path: 'old.ts' })], 1, 'codex'),
      ...mapWith([tool('Write', 'start', { path: 'src/a.ts' })], 2, 'codex'),
      ...mapWith([tool('Edit', 'end', { path: 'src/b.ts' })], 2, 'grok'),
    };
    expect(latestProcessTurn(processMap)).toBe(2);
    expect(extractTurnEdits(processMap).map((file) => file.path)).toEqual([
      'src/a.ts',
      'src/b.ts',
    ]);
    expect(extractTurnEdits(processMap, 1)).toEqual([
      { path: 'old.ts', status: 'done' },
    ]);
  });

  it('returns an empty list when there is no process map', () => {
    expect(extractTurnEdits({})).toEqual([]);
    expect(latestProcessTurn({})).toBeNull();
    expect(latestProcessTurn({
      'x:codex': {
        agent: 'codex',
        phase: 'ok',
        stdout: '',
        stderr: '',
        updatedAt: 1,
        steps: [],
      } as unknown as ProcessMap[string],
    })).toBeNull();
  });
});

describe('simple diff', () => {
  it('marks the changed middle lines', () => {
    expect(formatSimpleDiff('keep\nold\nend', 'keep\nnew\nend', 'src/a.ts')).toBe(
      [
        '--- src/a.ts',
        '+++ src/a.ts',
        '@@ -2,1 +2,1 @@',
        '-old',
        '+new',
      ].join('\n'),
    );
  });

  it('sameEditPath treats slash variants as one file', () => {
    expect(sameEditPath('src\\a.ts', 'src/a.ts')).toBe(true);
    expect(sameEditPath('src/a.ts', 'src/b.ts')).toBe(false);
    expect(sameEditPath('src/a.ts/', 'src/a.ts')).toBe(true);
    expect(sameEditPath('  src/a.ts  ', 'src/a.ts')).toBe(true);
  });

  it('turnEditDiffText prefers a real patch over inventing one', () => {
    expect(
      turnEditDiffText({
        path: 'README.md',
        diff: '@@ -1 +1,2 @@\n hello\n+world\n',
      }),
    ).toContain('@@ -1 +1,2 @@');
    expect(
      turnEditDiffText({
        path: 'a.ts',
        before: 'a',
        after: 'b',
      }),
    ).toContain('-a');
    expect(
      turnEditDiffText({
        path: 'a.ts',
        after: 'only-new',
      }),
    ).toBeNull();
  });
});

describe('ChatTurnEditList', () => {
  it('labels this turn\'s files with 查看修改 / 正在修改 / 已修改', () => {
    const html = renderToStaticMarkup(
      createElement(
        TooltipProvider,
        null,
        createElement(ChatTurnEditList, {
          files: [
            { path: 'src/a.ts', status: 'live' },
            { path: 'src/b.ts', status: 'done' },
          ],
          selectedPath: 'src\\a.ts',
          onSelect: () => undefined,
        }),
      ),
    );
    expect(html).toContain('data-help="chat-turn-edits"');
    expect(html).toContain('查看修改');
    expect(html).toContain('正在修改');
    expect(html).toContain('已修改');
    expect(html).toContain('src/a.ts');
    expect(html).toContain('src/b.ts');
    expect(html).toContain('aria-current="true"');
  });

  it('draws nothing when this turn has no edited files', () => {
    expect(
      renderToStaticMarkup(
        createElement(ChatTurnEditList, { files: [], onSelect: () => undefined }),
      ),
    ).toBe('');
  });
});

describe('ChatEditPreviewPanel', () => {
  it('returns nothing when the pane is closed', () => {
    expect(
      renderToStaticMarkup(
        createElement(ChatEditPreviewPanel, {
          file: { path: 'src/a.ts', status: 'done', before: 'a', after: 'b' },
          open: false,
          onClose: () => undefined,
        }),
      ),
    ).toBe('');
  });

  it('renders a diff preview from old and new text', () => {
    const html = renderToStaticMarkup(
      createElement(
        TooltipProvider,
        null,
        createElement(ChatEditPreviewPanel, {
          file: { path: 'src/app.ts', status: 'done', before: 'old', after: 'new' },
          open: true,
          width: 360,
          onClose: () => undefined,
        }),
      ),
    );
    expect(html).toContain('data-chat-edit-preview');
    expect(html).toContain('app.ts');
    expect(html).toContain('查看修改');
    expect(html).toContain('PREVIEW:app.ts.diff:');
    expect(html).toContain('-old');
    expect(html).toContain('+new');
    expect(html).toContain('收起');
  });

  it('shows the empty-body hint when there is no patch', () => {
    const html = renderToStaticMarkup(
      createElement(
        TooltipProvider,
        null,
        createElement(ChatEditPreviewPanel, {
          file: { path: 'src/a.ts', status: 'live' },
          open: true,
          onClose: () => undefined,
        }),
      ),
    );
    expect(html).toContain('没有内容');
    expect(html).not.toContain('PREVIEW:');
  });
});
