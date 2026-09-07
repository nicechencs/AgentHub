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

  it('does not pin a one-shot banner; composer still names allow/deny and queue', () => {
    expect(kiroChatStance('claude')).toBeNull();
    expect(kiroChatStance('cursor')).toBeNull();
    expect(kiroChatStance('kiro')).toBeNull();
    expect(kiroChatBannerCopy(t)).toEqual({
      title: t('chat.kiro.oneshotHint'),
      detail: t('chat.kiro.oneshotDetail'),
    });
    expect(kiroChatComposerPlaceholder(t, 'kiro', 'fallback')).toBe(t('chat.kiro.placeholder'));
    expect(kiroChatComposerPlaceholder(t, 'claude', 'fallback')).toBe('fallback');
    expect(t('chat.kiro.oneshotHint')).toContain('允许/拒绝');
    expect(t('chat.kiro.oneshotHint')).toContain('不能中途补充');
    expect(t('chat.kiro.oneshotDetail')).toContain('请新建对话');
    expect(t('chat.kiro.oneshotDetail')).toContain('不在本页');
    expect(t('chat.kiro.placeholder')).toContain('允许/拒绝');
    expect(t('chat.kiro.placeholder')).toContain('排队');
  });

  it('keeps runtime request panels and command search on for Kiro', () => {
    expect(kiroChatAllowsCommandSearch('kiro')).toBe(true);
    expect(kiroChatAllowsCommandSearch('codex')).toBe(true);
    expect(chatShowsRuntimeRequestPanels('kiro')).toBe(true);
    expect(chatShowsRuntimeRequestPanels('codex')).toBe(true);
    expect(chatShowsRuntimeRequestPanels('grok')).toBe(true);
    expect(chatShowsRuntimeRequestPanels('cursor')).toBe(false);
    expect(chatComposerChoiceOptions('kiro', ['sonnet', 'haiku'])).toEqual(['sonnet', 'haiku']);
    expect(chatComposerChoiceOptions('claude', ['sonnet', 'haiku'])).toEqual(['sonnet', 'haiku']);
  });
});
