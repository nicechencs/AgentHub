import { createElement, type ReactElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { TooltipProvider } from '@/components/ui/tooltip';
import type { Conversation } from '@/lib/types';
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
    hiddenIds: new Set(),
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

describe('ChatComposer empty invite', () => {
  it('invites typing in the placeholder and keeps Kiro limits on the quiet hint', () => {
    const html = renderMarkup(composer());
    expect(html).toContain('向 Kiro 发第一条消息…');
    expect(html).toContain('data-help="chat-composer-hint"');
    expect(html).toContain('生成时不能中途补充，可排队到下一轮。');
    expect(html).toContain('aria-label="消息输入"');
    expect(html).not.toMatch(/placeholder="[^"]*不能中途补充/);
  });

  it('uses the everyday placeholder after the first turn', () => {
    const html = renderMarkup(composer({ emptyTranscript: false, primaryAgent: 'codex', agentPickerLabel: 'Codex' }));
    expect(html).toContain('发给 Agent…');
    expect(html).not.toContain('data-help="chat-composer-hint"');
  });
});
