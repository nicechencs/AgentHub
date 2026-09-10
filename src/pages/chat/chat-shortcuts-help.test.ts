import { describe, expect, it } from 'vitest';
import { translate } from '@/lib/i18n';
import {
  shortcutsHelpExpanded,
  shortcutsHelpOpenChange,
} from './chat-shortcuts-help';

describe('shortcuts help open state', () => {
  it('opens on hover and stays pinned after click', () => {
    expect(shortcutsHelpExpanded(null)).toBe(false);
    const hovered = shortcutsHelpOpenChange(null, 'hover-enter');
    expect(hovered).toBe('hover');
    expect(shortcutsHelpExpanded(hovered)).toBe(true);
    expect(shortcutsHelpOpenChange(hovered, 'hover-leave')).toBeNull();

    const pinned = shortcutsHelpOpenChange(hovered, 'click');
    expect(pinned).toBe('click');
    expect(shortcutsHelpOpenChange(pinned, 'hover-leave')).toBe('click');
    expect(shortcutsHelpOpenChange(pinned, 'click')).toBeNull();
    expect(shortcutsHelpOpenChange(pinned, 'dismiss')).toBeNull();
  });

  it('reuses the existing shortcut overview words', () => {
    expect(translate('zh', 'chat.shortcuts.open')).toBe('快捷键');
    expect(translate('en', 'chat.shortcuts.open')).toBe('Shortcuts');
    expect(translate('zh', 'chat.shortcuts.overview')).toBe('快捷键一览');
    expect(translate('zh', 'chat.shortcuts.newChat')).toBe('新建对话');
    expect(translate('zh', 'chat.shortcuts.model')).toBe('换模型');
  });
});
