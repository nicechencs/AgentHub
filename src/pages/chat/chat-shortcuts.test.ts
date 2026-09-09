import { describe, expect, it } from 'vitest';
import { translate } from '@/lib/i18n';
import { CHAT_SHORTCUT_ROWS, chatShortcutChord } from './chat-shortcuts';

describe('chat shortcut overview', () => {
  it('lists new chat and the overview itself', () => {
    expect(CHAT_SHORTCUT_ROWS.map((row) => row.id)).toEqual([
      'send',
      'newline',
      'stop',
      'actions',
      'history',
      'model',
      'newChat',
      'overview',
    ]);
    expect(CHAT_SHORTCUT_ROWS.find((row) => row.id === 'newChat')?.keys).toBe('Ctrl+N');
    expect(CHAT_SHORTCUT_ROWS.find((row) => row.id === 'overview')?.keys).toBe('?');
  });

  it('shows Cmd on macOS and Ctrl elsewhere', () => {
    expect(chatShortcutChord('Ctrl+N', 'linux')).toBe('Ctrl+N');
    expect(chatShortcutChord('Ctrl+N', 'windows')).toBe('Ctrl+N');
    expect(chatShortcutChord('Ctrl+N', 'macos')).toBe('Cmd+N');
    expect(chatShortcutChord('Ctrl+Shift+I', 'macos')).toBe('Cmd+Shift+I');
    expect(chatShortcutChord('Enter', 'macos')).toBe('Enter');
  });

  it('uses existing Chat words in both languages', () => {
    expect(translate('zh', 'chat.shortcuts.newChat')).toBe('新建对话');
    expect(translate('en', 'chat.shortcuts.newChat')).toBe('New chat');
    expect(translate('zh', 'chat.shortcuts.open')).toBe('快捷键');
    expect(translate('en', 'chat.shortcuts.open')).toBe('Shortcuts');
    expect(translate('zh', 'chat.shortcuts.overview')).toBe('快捷键一览');
    expect(translate('zh', 'chat.shortcuts.ime')).toBe('组字时 Enter 不发送');
    expect(translate('en', 'chat.shortcuts.ime')).toBe('Enter does not send while composing');
  });
});
