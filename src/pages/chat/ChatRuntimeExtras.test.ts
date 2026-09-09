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

describe('ChatRuntimeExtras image chips', () => {
  it('shows removable image chips in inline composer mode', () => {
    const html = renderMarkup(
      extras({
        inline: true,
        images: ['/tmp/qa/ping.png', '/tmp/qa/shot.webp'],
        imageInput: true,
      }),
    );
    expect(html).toContain('ping.png');
    expect(html).toContain('shot.webp');
    expect(html).toContain('添加图片');
    expect(html).not.toContain('localImage');
    // Must not stay on display:contents when chips exist (true-window layout).
    expect(html).toContain('flex-col');
    expect(html).toContain('data-help="chat-image-chips"');
    expect(html).toMatch(/aria-label="[^"]*"/);
  });

  it('keeps contents layout in inline mode when there are no images', () => {
    const html = renderMarkup(extras({ inline: true, images: [] }));
    expect(html).toContain('contents');
    expect(html).not.toContain('data-help="chat-image-chips"');
  });
});



describe('ChatRuntimeExtras skill picker', () => {
  it('hides the skill picker when showSkillPicker is false', () => {
    const html = renderMarkup(
      extras({
        showSkillPicker: false,
        extensions: [
          { id: 'skill-a', kind: 'skill', name: 'Demo Skill', callable: true, installed: true, enabled: true, loaded: true },
        ],
        selectedSkillIds: ['skill-a'],
      }),
    );
    expect(html).not.toContain('Demo Skill');
    // Label "技能" from the picker button / chips should not appear as a control.
    expect(html).not.toMatch(/>\s*技能/);
  });

  it('hides the skill picker for Codex even when showSkillPicker is true', () => {
    const html = renderMarkup(
      extras({
        agentId: 'codex',
        showSkillPicker: true,
        extensions: [
          { id: 'skill-a', kind: 'skill', name: 'Demo Skill', callable: true, installed: true, enabled: true, loaded: true },
        ],
        selectedSkillIds: ['skill-a'],
      }),
    );
    expect(html).not.toContain('Demo Skill');
    expect(html).not.toMatch(/>\s*技能/);
  });

  it('shows the skill picker by default when callable skills exist', () => {
    const html = renderMarkup(
      extras({
        extensions: [
          { id: 'skill-a', kind: 'skill', name: 'Demo Skill', callable: true, installed: true, enabled: true, loaded: true },
        ],
      }),
    );
    // Radix closed menus do not SSR item labels; the toolbar trigger is enough.
    expect(html).toContain('>技能<');
  });

  it('keeps first-use image and skill controls as icon-only with accessible names', () => {
    const html = renderMarkup(
      extras({
        compactSecondary: true,
        inline: true,
        imageInput: true,
        extensions: [
          { id: 'skill-a', kind: 'skill', name: 'Demo Skill', callable: true, installed: true, enabled: true, loaded: true },
        ],
      }),
    );
    expect(html).toContain('aria-label="添加图片"');
    expect(html).toContain('aria-label="技能"');
    expect(html).not.toMatch(/>添加图片</);
    expect(html).not.toMatch(/>技能</);
    expect(html).not.toMatch(/>可能更慢</);
  });
});
