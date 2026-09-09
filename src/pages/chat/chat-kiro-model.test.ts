import { describe, expect, it } from 'vitest';
import { createTranslator } from '@/lib/i18n';
import { isRuntimeChatAgent } from './chat-runtime-model';
import {
  chatComposerChoiceOptions,
  chatShowsRuntimeRequestPanels,
  isKiroChatAgent,
  isOneShotHeadlessChatAgent,
  kiroChatAllowsCommandSearch,
  kiroChatBannerCopy,
  kiroChatComposerPlaceholder,
  kiroChatStance,
} from './chat-kiro-model';

const t = createTranslator('zh');

describe('Kiro chat honesty helpers', () => {
  it('treats kiro as a continuous ACP agent, not a one-shot headless id', () => {
    expect(isKiroChatAgent('kiro')).toBe(true);
    expect(isKiroChatAgent('cursor')).toBe(false);
    expect(isKiroChatAgent(null)).toBe(false);
    expect(isOneShotHeadlessChatAgent('kiro')).toBe(false);
    expect(isOneShotHeadlessChatAgent('cursor')).toBe(true);
    expect(isOneShotHeadlessChatAgent('codex')).toBe(false);
    expect(isRuntimeChatAgent('kiro')).toBe(true);
    expect(isRuntimeChatAgent('cursor')).toBe(false);
  });

  it('does not pin a one-shot banner; composer invite stays off the warning', () => {
    expect(kiroChatStance('claude')).toBeNull();
    expect(kiroChatStance('cursor')).toBeNull();
    expect(kiroChatStance('kiro')).toBeNull();
    expect(kiroChatBannerCopy(t)).toEqual({
      title: t('chat.kiro.oneshotHint'),
      detail: t('chat.kiro.oneshotDetail'),
    });
    expect(kiroChatComposerPlaceholder(t, 'kiro', 'fallback')).toBe('fallback');
    expect(kiroChatComposerPlaceholder(t, 'claude', 'fallback')).toBe('fallback');
    expect(t('chat.kiro.oneshotHint')).toContain('允许、一直允许或拒绝');
    expect(t('chat.kiro.oneshotHint')).toContain('不能中途补充');
    expect(t('chat.kiro.oneshotDetail')).toContain('请新建对话');
    expect(t('chat.kiro.oneshotDetail')).toContain('不在本页');
    expect(t('chat.kiro.placeholder')).toBe('发给 Agent…');
    expect(t('chat.kiro.placeholder')).not.toContain('不能中途补充');
    expect(t('chat.kiro.permissionAsk')).toBe('帮我批准');
    expect(t('chat.kiro.permissionFull')).toBe('完全访问权限');
    expect(t('chat.kiro.settingsLocked')).toContain('权限');
    expect(t('chat.kiro.settingsLocked')).toContain('请新建对话');
  });

  it('keeps runtime request panels and command search on for Kiro', () => {
    expect(kiroChatAllowsCommandSearch('kiro')).toBe(true);
    expect(kiroChatAllowsCommandSearch('codex')).toBe(true);
    expect(kiroChatAllowsCommandSearch('pi')).toBe(true);
    expect(kiroChatAllowsCommandSearch('grok')).toBe(true);
    expect(chatShowsRuntimeRequestPanels('kiro')).toBe(true);
    expect(chatShowsRuntimeRequestPanels('codex')).toBe(true);
    expect(chatShowsRuntimeRequestPanels('grok')).toBe(true);
    expect(chatShowsRuntimeRequestPanels('cursor')).toBe(false);
    expect(chatComposerChoiceOptions('kiro', ['sonnet', 'haiku'])).toEqual(['sonnet', 'haiku']);
    expect(chatComposerChoiceOptions('claude', ['sonnet', 'haiku'])).toEqual(['sonnet', 'haiku']);
  });
});
