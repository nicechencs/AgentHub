import { readFileSync } from 'node:fs';
import { createElement, type ReactElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { TooltipProvider } from '@/components/ui/tooltip';
import type { Conversation } from '@/lib/types';
import { createTranslator } from '@/lib/i18n';
import {
  conversationRailHintView,
  conversationSemanticTitle,
  type ConversationWorkspaceGroup,
} from './chat-model';
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

function workspaceGroup(
  items: Conversation[],
  partial?: Partial<ConversationWorkspaceGroup>,
): ConversationWorkspaceGroup {
  const cwd = items[0]?.cwd ?? null;
  return {
    key: cwd ? `path:${cwd}` : 'unset',
    label: cwd ? 'demo-project' : '未设置工作目录',
    cwd,
    items,
    ...partial,
  };
}

function rail(partial?: Partial<Parameters<typeof ChatSessionRail>[0]>) {
  const item = conversation();
  return createElement(ChatSessionRail, {
    open: true,
    listLoading: false,
    groups: [workspaceGroup([item])],
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
        groups: [workspaceGroup([conversation({ title: full })])],
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
        groups: [workspaceGroup([active, clipped])],
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

  it('uses a single-line semantic title and keeps cwd only in the hover helper', () => {
    const html = renderMarkup(rail());
    const titleAt = html.indexOf('data-help="chat-session-title"');
    expect(titleAt).toBeGreaterThan(0);
    const titleSlice = html.slice(titleAt, titleAt + 180);
    expect(titleSlice).toContain('检查问题');
    expect(titleSlice).not.toContain('/workspace/src/app.ts');
    expect(html).not.toContain('/workspace/src/app.ts');
    const buttonAt = html.indexOf('data-session-id="c1"');
    const buttonEnd = html.indexOf('</button>', buttonAt);
    expect(buttonAt).toBeGreaterThan(0);
    expect(buttonEnd).toBeGreaterThan(buttonAt);
    const buttonHtml = html.slice(buttonAt, buttonEnd);
    expect(buttonHtml).toContain('data-help="chat-session-title"');
    expect(buttonHtml).not.toContain('demo-project');
    expect(buttonHtml).not.toContain('text-meta text-muted');
    const hint = conversationRailHintView(conversation(), createTranslator('zh'));
    expect(hint.meta).toContain('/workspace/demo-project');
    const src = readFileSync(new URL('./ChatSessionRail.tsx', import.meta.url), 'utf8');
    expect(src).toContain('conversationRailHintView(');
    expect(src).toContain('data-help="chat-workspace-group-label"');
  });

  it('groups sessions under a collapsible working-directory header', () => {
    const html = renderMarkup(rail());
    expect(html).toContain('data-help="chat-workspace-group"');
    expect(html).toContain('data-help="chat-workspace-group-label"');
    expect(html).toContain('demo-project');
    expect(html).toContain('aria-expanded="true"');
    expect(html).not.toContain('今天');
    const src = readFileSync(new URL('./ChatSessionRail.tsx', import.meta.url), 'utf8');
    expect(src).toContain('ChevronDown');
    expect(src).toContain('ChevronRight');
  });

  it('hides a plus on the folder until hover, then starts a chat in that folder', () => {
    const html = renderMarkup(rail());
    const src = readFileSync(new URL('./ChatSessionRail.tsx', import.meta.url), 'utf8');
    expect(html).toContain('data-help="chat-workspace-new"');
    expect(src).toContain('group-hover:opacity-100');
    expect(src).toContain('onNewChat(group.cwd)');
    expect(html).toContain('opacity-0');
  });

  it('puts the folder path on hover, not in the group header', () => {
    const item = conversation();
    const html = renderMarkup(rail({ groups: [workspaceGroup([item])] }));
    const src = readFileSync(new URL('./ChatSessionRail.tsx', import.meta.url), 'utf8');
    expect(src).toContain('Hint label={group.cwd ?? group.label}');
    expect(src).not.toContain('chat-workspace-group-path');
    expect(html).toContain('data-help="chat-workspace-group-label"');
    expect(html).toContain('demo-project');
    const labelStart = html.indexOf('data-help="chat-workspace-group-label"');
    const labelHtml = html.slice(labelStart, labelStart + 180);
    expect(labelHtml).toContain('demo-project');
    expect(labelHtml).not.toContain('/workspace/demo-project');
  });

  it('paints 新建对话 with the theme fill', () => {
    const html = renderMarkup(rail());
    expect(html).toContain('data-help="chat-new"');
    expect(html).toContain('data-btn="default"');
    expect(html).toContain('bg-accent');
    expect(html).not.toContain('删除确认 Enter');
  });

  it('keeps two working-directory groups and an unset group on separate headers', () => {
    const app = conversation({ id: 'app', cwd: '/workspace/demo-project', title: '修登录' });
    const other = conversation({ id: 'other', cwd: '/tmp/other', title: '另一场' });
    const unset = conversation({ id: 'unset', cwd: null, title: '未设' });
    const html = renderMarkup(
      rail({
        groups: [
          workspaceGroup([app]),
          workspaceGroup([other], { key: 'path:/tmp/other', label: 'other', cwd: '/tmp/other' }),
          workspaceGroup([unset], { key: 'unset', label: '未设置工作目录', cwd: null }),
        ],
        conversations: [app, other, unset],
        filteredCount: 3,
      }),
    );
    expect(html.split('data-help="chat-workspace-group"')).toHaveLength(4);
    expect(html).toContain('demo-project');
    expect(html).toContain('other');
    expect(html).toContain('未设置工作目录');
    expect(html).toContain('data-session-id="app"');
    expect(html).toContain('data-session-id="other"');
    expect(html).toContain('data-session-id="unset"');
    expect(html.split('data-help="chat-workspace-new"')).toHaveLength(3);
    const unsetAt = html.indexOf('data-session-id="unset"');
    const unsetGroup = html.slice(html.lastIndexOf('data-help="chat-workspace-group"', unsetAt), unsetAt);
    expect(unsetGroup).not.toContain('data-help="chat-workspace-new"');
  });
});

