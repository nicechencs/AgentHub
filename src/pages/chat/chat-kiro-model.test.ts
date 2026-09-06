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
  it('treats kiro as a half-surface id, not a ChatRuntime continuous agent', () => {
    expect(isKiroChatAgent('kiro')).toBe(true);
    expect(isKiroChatAgent('cursor')).toBe(false);
    expect(isKiroChatAgent(null)).toBe(false);
    expect(isOneShotHeadlessChatAgent('kiro')).toBe(true);
    expect(isOneShotHeadlessChatAgent('cursor')).toBe(true);
    expect(isOneShotHeadlessChatAgent('codex')).toBe(false);
    expect(isRuntimeChatAgent('kiro')).toBe(false);
    expect(isRuntimeChatAgent('cursor')).toBe(false);
  });

  it('exposes one-shot stance copy and composer gates only for kiro', () => {
    expect(kiroChatStance('claude')).toBeNull();
    expect(kiroChatStance('cursor')).toBeNull();
    expect(kiroChatStance('kiro')).toEqual({
      showBanner: true,
      allowCommandSearch: false,
      allowRuntimeRequests: false,
      allowModelPicker: false,
      allowSteer: false,
      allowQueueFollowUp: false,
    });
    expect(kiroChatBannerCopy(t)).toEqual({
      title: t('chat.kiro.oneshotHint'),
      detail: t('chat.kiro.oneshotDetail'),
    });
    expect(kiroChatComposerPlaceholder(t, 'kiro', 'fallback')).toBe(t('chat.kiro.placeholder'));
    expect(kiroChatComposerPlaceholder(t, 'claude', 'fallback')).toBe('fallback');
    expect(t('chat.kiro.oneshotHint')).toContain('本机登录');
    expect(t('chat.kiro.oneshotHint')).toContain('API Key');
    expect(t('chat.kiro.oneshotHint')).toContain('一轮一发');
    expect(t('chat.kiro.oneshotDetail')).toContain('不能中途补充');
    expect(t('chat.kiro.oneshotDetail')).toContain('不在本页');
    expect(t('chat.kiro.oneshotDetail')).toContain('下一轮');
    expect(t('chat.kiro.placeholder')).toContain('下一轮');
  });

  it('does not keep slash pickers, model chips, or runtime request panels', () => {
    expect(kiroChatAllowsCommandSearch('kiro')).toBe(false);
    expect(kiroChatAllowsCommandSearch('codex')).toBe(true);
    expect(chatShowsRuntimeRequestPanels('kiro')).toBe(false);
    expect(chatShowsRuntimeRequestPanels('codex')).toBe(true);
    expect(chatShowsRuntimeRequestPanels('grok')).toBe(true);
    expect(chatShowsRuntimeRequestPanels('cursor')).toBe(false);
    expect(chatComposerChoiceOptions('kiro', ['sonnet', 'haiku'])).toEqual([]);
    expect(chatComposerChoiceOptions('claude', ['sonnet', 'haiku'])).toEqual(['sonnet', 'haiku']);
  });
});
