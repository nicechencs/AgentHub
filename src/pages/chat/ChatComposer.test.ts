import { createElement, type ReactElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { TooltipProvider } from '@/components/ui/tooltip';
import type { AgentKey, Conversation } from '@/lib/types';
import { ChatComposer } from './ChatComposer';

vi.mock('react-router-dom', () => ({
  useNavigate: () => () => undefined,
}));

vi.mock('@/components/shared/LanguageProvider', async () => {
  const { createTranslator } = await import('@/lib/i18n');
  const t = createTranslator('zh');
  return {
    useI18n: () => ({ lang: 'zh', setLanguage: () => undefined, t }),
  };
});

function conversation(): Conversation {
  return {
    id: 'c1',
    title: '新对话',
    agentIds: ['kiro'],
    cwd: 'D:\\demo',
    allowDangerous: false,
    createdAt: '2026-08-16T00:00:00.000Z',
    updatedAt: '2026-08-16T00:00:00.000Z',
    nativeSessionId: null,
  };
}

function renderMarkup(node: ReactElement) {
  return renderToStaticMarkup(createElement(TooltipProvider, null, node));
}

function composer(partial?: Partial<Parameters<typeof ChatComposer>[0]>) {
  return createElement(ChatComposer, {
    draft: '',
    setDraft: () => undefined,
    sending: false,
    active: conversation(),
    connectionOptions: [],
    primaryAgent: 'kiro',
    agentPickerLabel: 'Kiro',
    connectionView: {
      kind: 'none',
      label: '本机',
      subtitle: null,
      currentLoginTitle: null,
      currentLoginSubtitle: null,
      emptyHint: null,
      manageLabel: '管理',
    },
    switchingProvider: false,
    hiddenIds: new Set<AgentKey>(),
    pickerRows: [],
    agentsReady: true,
    blockers: [],
    connectionCaption: null,
    onSend: () => undefined,
    onCancel: () => undefined,
    onSelectAgent: () => undefined,
    onSwitchConnection: () => undefined,
    modelOptions: [],
    currentModel: null,
    switchingModel: false,
    onSwitchModel: () => undefined,
    onPickWorkingDirectory: () => undefined,
    emptyTranscript: true,
    ...partial,
  });
}

describe('ChatComposer footer control', () => {
  it('shows only Send when idle', () => {
    const html = renderMarkup(composer({ draft: '', sending: false }));
    expect(html).toContain('data-help="chat-send"');
    expect(html).not.toContain('data-help="chat-stop"');
    expect(html).toContain('aria-label="发送"');
    expect(html).toContain('disabled');
  });

  it('shows only Send while generating when the draft can queue', () => {
    const html = renderMarkup(
      composer({
        draft: '是否',
        sending: true,
        onQueueAfterTurn: () => undefined,
      }),
    );
    expect(html).toContain('data-help="chat-send"');
    expect(html).not.toContain('data-help="chat-stop"');
    expect(html).toContain('aria-label="本轮结束后发送"');
    expect(html).toContain('h-8 w-8 shrink-0 rounded-full');
  });

  it('shows Stop when generating has text but no inject or queue channel', () => {
    const html = renderMarkup(composer({ draft: '是否', sending: true }));
    expect(html).toContain('data-help="chat-stop"');
    expect(html).not.toContain('data-help="chat-send"');
  });

  it('shows only Stop in the same slot while generating and the draft is empty', () => {
    const html = renderMarkup(
      composer({
        draft: '',
        sending: true,
        onQueueAfterTurn: () => undefined,
      }),
    );
    expect(html).toContain('data-help="chat-stop"');
    expect(html).not.toContain('data-help="chat-send"');
    expect(html).toContain('aria-label="停止"');
    expect(html).toContain('aria-keyshortcuts="Escape"');
    expect(html).toContain('停止 · Esc');
    expect(html).toContain('h-8 w-8 shrink-0 rounded-full');
    expect(html).not.toContain('>停止<');
  });

  it('keeps 正在停止 on the same circular Stop while cancelling', () => {
    const html = renderMarkup(
      composer({
        draft: '',
        sending: true,
        canceling: true,
        onQueueAfterTurn: () => undefined,
      }),
    );
    expect(html).toContain('data-help="chat-stop"');
    expect(html).not.toContain('data-help="chat-send"');
    expect(html).toContain('aria-label="正在停止"');
    expect(html).toContain('aria-busy="true"');
    expect(html).toContain('disabled');
    expect(html).not.toContain('正在停止 · Esc');
  });
});

describe('ChatComposer empty invite', () => {
  it('uses a generic placeholder and keeps limits on hover titles', () => {
    const html = renderMarkup(composer());
    expect(html).toContain('发消息…');
    expect(html).not.toContain('向 Kiro');
    expect(html).not.toContain('data-help="chat-composer-hint"');
    expect(html).not.toContain('>生成时不能中途补充，可排队到下一轮。<');
    expect(html).toContain('生成时不能中途补充，可排队到下一轮。');
    expect(html).toContain('aria-label="消息输入"');
    expect(html).toContain('data-help="chat-shortcuts"');
    expect(html).toContain('aria-expanded="false"');
    expect(html).not.toMatch(/placeholder="[^"]*不能中途补充/);
    expect(html).not.toMatch(/>快捷键</);
  });

  it('uses the everyday placeholder after the first turn and restores the shortcut line', () => {
    const html = renderMarkup(composer({ emptyTranscript: false, primaryAgent: 'codex', agentPickerLabel: 'Codex' }));
    expect(html).toContain('发给 Agent…');
    expect(html).toContain('Enter 发送 · Shift+Enter 换行');
    expect(html).not.toContain('data-help="chat-composer-hint"');
  });
});
