import { readFileSync } from 'node:fs';
import { createElement, type ReactElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { TooltipProvider } from '@/components/ui/tooltip';
import type { Conversation } from '@/lib/types';
import { createTranslator } from '@/lib/i18n';
import { conversationRailHintView, conversationSemanticTitle } from './chat-model';
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
  it('keeps the list line display-clipped and wires hover to the full title helper', () => {
    const full =
      'Use your terminal to write exactly what I asked without clipping the title';
    const html = renderMarkup(
      rail({
        groups: [{ key: 'today', label: '今天', items: [conversation({ title: full })] }],
        conversations: [conversation({ title: full })],
        firstUserContentById: { c1: full },
      }),
    );
    expect(html).toContain('data-help="chat-session-title"');
    expect(html).toContain(conversationSemanticTitle(full));
    const src = readFileSync(new URL('./ChatSessionRail.tsx', import.meta.url), 'utf8');
    expect(src).toContain('conversationRailHintView(');
    expect(src).toContain('firstUserContent');
    expect(src).toContain('data-help="chat-session-hint-title"');
    expect(src).toContain('{hint.title}');
    expect(src).not.toMatch(/function ConversationRailHintLabel[\s\S]*AgentLogo/);
  });

  it('recovers a clipped hover title for a non-active row from list first-user content', () => {
    const prompt =
      'Use your terminal to write exactly what I asked without clipping the title';
    const stored = `${prompt.slice(0, 24)}…`;
    const active = conversation({ id: 'active', title: '当前会话' });
    const clipped = conversation({
      id: 'clipped',
      title: stored,
      firstUserContent: prompt,
    });
    const firstUserContentById: Record<string, string> = {};
    renderMarkup(
      rail({
        activeId: active.id,
        groups: [{ key: 'today', label: '今天', items: [active, clipped] }],
        conversations: [active, clipped],
        filteredCount: 2,
        firstUserContentById,
      }),
    );
    const firstUserContent = firstUserContentById[clipped.id] ?? clipped.firstUserContent;
    expect(firstUserContent).toBe(prompt);
    const hint = conversationRailHintView(
      { ...clipped, firstUserContent },
      createTranslator('zh'),
    );
    expect(hint.title).toBe(prompt);
    expect(hint.title).not.toMatch(/…|\.\.\./);
    const src = readFileSync(new URL('./ChatSessionRail.tsx', import.meta.url), 'utf8');
    expect(src).toContain('firstUserContentById?.[c.id] ?? c.firstUserContent');
    expect(src).not.toMatch(/function ConversationRailHintLabel[\s\S]*AgentLogo/);
  });

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

