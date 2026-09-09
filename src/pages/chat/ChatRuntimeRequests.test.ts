import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createElement, type ReactElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import type { RuntimeRequest } from '@/lib/api/chat';
import { ChatRuntimeRequests } from './ChatRuntimeRequests';
import { runtimeFileChangePreview } from './chat-runtime-model';

const fixtureDir = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  '../../../crates/agenthub-core/src/services/chat_runtime/file_change/fixtures',
);

function protocolFixture(name: string): Record<string, unknown> {
  return JSON.parse(readFileSync(path.join(fixtureDir, name), 'utf8')) as Record<string, unknown>;
}

vi.mock('@/components/shared/LanguageProvider', async () => {
  const { createTranslator } = await import('@/lib/i18n');
  const t = createTranslator('zh');
  return {
    useI18n: () => ({ lang: 'zh', setLanguage: () => undefined, t }),
  };
});

function renderMarkup(node: ReactElement) {
  return renderToStaticMarkup(node);
}

const command: RuntimeRequest = {
  id: 'req-1',
  runId: 'run-1',
  kind: 'command',
  title: 'execute',
  detail: 'ls',
  questions: [],
  permissionOptions: [
    { id: 'once', kind: 'allow_once' },
    { id: 'always', kind: 'allow_always' },
  ],
};

describe('runtime allow/deny card copy', () => {
  it('puts the process-only hint next to Always allow', () => {
    const html = renderMarkup(
      createElement(ChatRuntimeRequests, {
        requests: [command],
        agentId: 'grok',
        onReply: async () => undefined,
      }),
    );
    expect(html).toContain('data-help="chat-allow-always"');
    expect(html).toContain('一直允许');
    expect(html).toContain('仅当前这次对话，不保存');
    expect(html).not.toContain('通常只记到本轮');
    const alwaysAt = html.indexOf('一直允许');
    const hintAt = html.indexOf('仅当前这次对话，不保存');
    const denyAt = html.indexOf('拒绝');
    expect(alwaysAt).toBeGreaterThan(0);
    expect(hintAt).toBeGreaterThan(alwaysAt);
    expect(denyAt).toBeGreaterThan(hintAt);
  });

  it('says Codex remember lasts this conversation', () => {
    const html = renderMarkup(
      createElement(ChatRuntimeRequests, {
        requests: [command],
        agentId: 'codex',
        onReply: async () => undefined,
      }),
    );
    expect(html).toContain('仅当前这次对话，不保存');
    expect(html).not.toContain('通常只记到本轮');
  });

  it('does not invent Always allow when the request has no such option', () => {
    const html = renderMarkup(
      createElement(ChatRuntimeRequests, {
        requests: [{ ...command, permissionOptions: [{ id: 'once', kind: 'allow_once' }] }],
        agentId: 'codex',
        onReply: async () => undefined,
      }),
    );
    expect(html).not.toContain('一直允许');
    expect(html).not.toContain('仅当前这次对话');
  });
});

function fileRequest(fileChanges: RuntimeRequest['fileChanges'], detail: string): RuntimeRequest {
  return {
    id: 'file-1',
    runId: 'run-1',
    kind: 'file',
    title: '修改文件',
    detail,
    questions: [],
    fileChanges,
  };
}

describe('file change approval preview', () => {
  it('renders the protocol diff from the Codex item/started fixture payload', () => {
    const html = renderMarkup(
      createElement(ChatRuntimeRequests, {
        requests: [fileRequest([{
          path: '/workspace/qa-codex-filechange-scratch/probe.txt',
          kind: 'add',
          preview: 'FILECHANGE_OK\n',
        }], '/workspace/qa-codex-filechange-scratch/probe.txt')],
        agentId: 'codex',
        onReply: async () => undefined,
      }),
    );
    expect(html).toContain('修改文件');
    expect(html).toContain('data-help="chat-file-change-preview"');
    expect(html).toContain('FILECHANGE_OK');
    expect(html).toContain('/workspace/qa-codex-filechange-scratch/probe.txt');
    expect(html).toContain('新增');
    expect(html).not.toContain('暂无改动预览');
  });

  it('renders apply_patch content from the fixture payload', () => {
    const html = renderMarkup(
      createElement(ChatRuntimeRequests, {
        requests: [fileRequest([{
          path: '/tmp/example.txt',
          kind: 'add',
          preview: 'ok',
        }], '/tmp/example.txt')],
        agentId: 'codex',
        onReply: async () => undefined,
      }),
    );
    expect(html).toContain('ok');
    expect(html).toContain('/tmp/example.txt');
  });

  it('reads preview fields from the checked-in protocol fixtures', () => {
    const started = protocolFixture('item_started_with_diff.json') as {
      item: { changes: Array<{ path: string; diff: string }> };
    };
    expect(started.item.changes[0].diff).toBe('FILECHANGE_OK\n');
    const applyPatch = protocolFixture('apply_patch_with_content.json') as {
      fileChanges: Record<string, { content: string }>;
    };
    expect(applyPatch.fileChanges['/tmp/example.txt'].content).toBe('ok');
    const pathOnly = protocolFixture('item_started_path_only.json') as {
      item: { changes: Array<{ path: string; diff?: string }> };
    };
    expect(pathOnly.item.changes[0].path).toBe('/workspace/notes.md');
    expect(pathOnly.item.changes[0].diff).toBeUndefined();
    const preview = runtimeFileChangePreview({
      kind: 'file',
      detail: started.item.changes[0].path,
      fileChanges: [{
        path: started.item.changes[0].path,
        kind: 'add',
        preview: started.item.changes[0].diff,
      }],
    });
    expect(preview.shown).toBe(true);
    expect(preview.shown && preview.empty).toBe(false);
  });

  it('shows an honest empty state when the fixture only has a path', () => {
    const html = renderMarkup(
      createElement(ChatRuntimeRequests, {
        requests: [fileRequest([{ path: '/workspace/notes.md', kind: 'update' }], '/workspace/notes.md')],
        agentId: 'codex',
        onReply: async () => undefined,
      }),
    );
    expect(html).toContain('data-help="chat-file-change-preview-empty"');
    expect(html).toContain('/workspace/notes.md');
    expect(html).toContain('暂无改动预览');
    expect(html).not.toContain('@@');
  });
});
