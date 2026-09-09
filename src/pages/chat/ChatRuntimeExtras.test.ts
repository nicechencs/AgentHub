import { createElement, type ReactElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { TooltipProvider } from '@/components/ui/tooltip';
import type { RuntimeModelOption, RuntimeTurnSettings } from '@/lib/api/chat';
import { ChatRuntimeExtras } from './ChatRuntimeExtras';

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

const models: RuntimeModelOption[] = [
  { id: 'gpt-mock', efforts: ['low', 'medium', 'high'], defaultEffort: 'medium' },
  { id: 'gpt-5.3-codex-spark', efforts: ['low', 'high', 'xhigh'], defaultEffort: 'high' },
];

const settings: RuntimeTurnSettings = { model: 'gpt-5.3-codex-spark', effort: 'high' };

function extras(partial?: Partial<Parameters<typeof ChatRuntimeExtras>[0]>) {
  return createElement(ChatRuntimeExtras, {
    enabled: true,
    draft: '',
    commandSearchOpen: false,
    actionContext: { hasLatestReply: false, newChatAllowed: true },
    onRunAction: () => undefined,
    models,
    settings,
    frozen: false,
    efforts: ['low', 'high', 'xhigh'],
    onSwitchModel: () => undefined,
    onSwitchEffort: () => undefined,
    images: [],
    onAddImages: () => undefined,
    onRemoveImage: () => undefined,
    extensions: [],
    selectedSkillIds: [],
    onToggleSkill: () => undefined,
    ...partial,
  });
}

describe('ChatRuntimeExtras model and effort labels', () => {
  it('shows a readable current model and a short thinking-effort hint', () => {
    const html = renderMarkup(extras());
    expect(html).toContain('GPT 5.3 Codex Spark');
    expect(html).not.toContain('>gpt-5.3-codex-spark<');
    expect(html).toContain('高');
    expect(html).toContain('可能更慢');
    expect(html).not.toContain('>high<');
    expect(html).toContain('data-help="chat-model"');
    expect(html).toContain('data-help="chat-effort"');
    expect(html).toContain('Control+Shift+I');
  });
});
