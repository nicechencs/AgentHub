import { createElement, type ReactElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import type { RuntimeRequest } from '@/lib/api/chat';
import { ChatRuntimeRequests } from './ChatRuntimeRequests';

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
    expect(html).toContain('仅当前这次进程，不保存');
    expect(html).not.toContain('通常只记到本轮');
    const alwaysAt = html.indexOf('一直允许');
    const hintAt = html.indexOf('仅当前这次进程，不保存');
    const denyAt = html.indexOf('拒绝');
    expect(alwaysAt).toBeGreaterThan(0);
    expect(hintAt).toBeGreaterThan(alwaysAt);
    expect(denyAt).toBeGreaterThan(hintAt);
  });

  it('says Codex remember is usually this turn', () => {
    const html = renderMarkup(
      createElement(ChatRuntimeRequests, {
        requests: [command],
        agentId: 'codex',
        onReply: async () => undefined,
      }),
    );
    expect(html).toContain('仅当前这次进程，通常只记到本轮，不保存');
    expect(html).not.toContain('>仅当前这次进程，不保存<');
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
    expect(html).not.toContain('仅当前这次进程');
  });
});
