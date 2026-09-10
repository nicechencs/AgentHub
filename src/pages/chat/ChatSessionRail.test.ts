import { createElement, type ReactElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { TooltipProvider } from '@/components/ui/tooltip';
import type { Conversation } from '@/lib/types';
import { ChatSessionRail } from './ChatSessionRail';

vi.mock('@/components/shared/LanguageProvider', async () => {
  const { createTranslator } = await import('@/lib/i18n');
  const t = createTranslator('zh');
  return {
    useI18n: () => ({ lang: 'zh', setLanguage: () => undefined, t }),
  };
});

function conversation(partial?: Partial<Conversation>): Conversation {
  return {
    id: 'c1',
    title: '请在 /workspace/src/app.ts 检查问题',
    agentIds: ['codex'],
    cwd: '/workspace/demo-project',
    allowDangerous: false,
    createdAt: '2026-09-09T00:00:00.000Z',
    updatedAt: '2026-09-09T00:00:00.000Z',
    nativeSessionId: 'sess-1',
    ...partial,
  };
}

function renderMarkup(node: ReactElement) {
  return renderToStaticMarkup(createElement(TooltipProvider, null, node));
}

function rail(partial?: Partial<Parameters<typeof ChatSessionRail>[0]>) {
  const item = conversation();
  return createElement(ChatSessionRail, {
    open: true,
    listLoading: false,
    groups: [{ key: 'today', label: '今天', items: [item] }],
    conversations: [item],
    filteredCount: 1,
    query: '',
    onQueryChange: () => undefined,
    activeId: item.id,
    sendingConversationIds: [],
    agentsReady: true,
    hasUsableAgent: true,
    deleteConfirmId: null,
    onToggleRail: () => undefined,
    onNewChat: () => undefined,
    onFocus: () => undefined,
    onRequestDelete: () => undefined,
    onCancelDelete: () => undefined,
    onConfirmDelete: () => undefined,
    ...partial,
  });
}

describe('ChatSessionRail titles', () => {
  it('uses a semantic title on the main line and cwd on the second line', () => {
    const html = renderMarkup(rail());
    const titleAt = html.indexOf('data-help="chat-session-title"');
    expect(titleAt).toBeGreaterThan(0);
    const titleSlice = html.slice(titleAt, titleAt + 180);
    expect(titleSlice).toContain('检查问题');
    expect(titleSlice).not.toContain('/workspace/src/app.ts');
    expect(html).not.toContain('/workspace/src/app.ts');
    expect(html).toContain('demo-project');
  });

  it('paints 新建对话 with the theme fill', () => {
    const html = renderMarkup(rail());
    expect(html).toContain('data-help="chat-new"');
    expect(html).toContain('data-btn="default"');
    expect(html).toContain('bg-accent');
    expect(html).not.toContain('删除确认 Enter');
  });
});

