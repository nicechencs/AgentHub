import { describe, expect, it } from 'vitest';
import { translate } from '@/lib/i18n';
import {
  composerEnterShouldSubmit,
  composerPrimaryAction,
  composerQueuedFollowUpView,
  composerShortcutKind,
  composerShortcutMessageKey,
  composerShouldRestoreFocus,
  composerShowsSubmitButton,
  composerStopMessageKey,
  composerSubmitMessageKey,
} from './chat-composer-model';

describe('composerEnterShouldSubmit', () => {
  it('sends on Enter and keeps Shift+Enter as a new line', () => {
    expect(composerEnterShouldSubmit({ key: 'Enter', shiftKey: false })).toBe(true);
    expect(composerEnterShouldSubmit({ key: 'Enter', shiftKey: true })).toBe(false);
    expect(composerEnterShouldSubmit({ key: 'a', shiftKey: false })).toBe(false);
  });

  it('does not send while IME is confirming', () => {
    expect(
      composerEnterShouldSubmit({ key: 'Enter', shiftKey: false, composing: true }),
    ).toBe(false);
    expect(
      composerEnterShouldSubmit({ key: 'Enter', shiftKey: false, keyCode: 229 }),
    ).toBe(false);
  });
});

describe('composer primary action honesty', () => {
  it('sends a non-empty unblocked draft', () => {
    expect(
      composerPrimaryAction({
        hasDraft: true,
        blocked: false,
        sending: false,
        canSteer: false,
        canQueue: false,
      }),
    ).toBe('send');
  });

  it('does not pretend to send when empty or blocked', () => {
    expect(
      composerPrimaryAction({
        hasDraft: false,
        blocked: false,
        sending: false,
        canSteer: false,
        canQueue: false,
      }),
    ).toBeNull();
    expect(
      composerPrimaryAction({
        hasDraft: true,
        blocked: true,
        sending: false,
        canSteer: false,
        canQueue: false,
      }),
    ).toBeNull();
  });

  it('injects only when a live steer channel exists', () => {
    expect(
      composerPrimaryAction({
        hasDraft: true,
        blocked: false,
        sending: true,
        canSteer: true,
        canQueue: true,
      }),
    ).toBe('steer');
    expect(
      composerPrimaryAction({
        hasDraft: true,
        blocked: false,
        sending: true,
        canSteer: false,
        canQueue: true,
      }),
    ).toBe('queue');
  });

  it('does not invent a send while generating without inject or queue', () => {
    expect(
      composerPrimaryAction({
        hasDraft: true,
        blocked: false,
        sending: true,
        canSteer: false,
        canQueue: false,
      }),
    ).toBeNull();
    expect(composerShowsSubmitButton({ sending: true, action: null })).toBe(false);
    expect(composerShowsSubmitButton({ sending: false, action: null })).toBe(true);
    expect(composerShowsSubmitButton({ sending: true, action: 'queue' })).toBe(true);
  });
});

describe('composer shortcut and stop copy', () => {
  it('names Enter / Shift+Enter for the current real action', () => {
    expect(
      composerShortcutKind({
        blocked: false,
        sending: false,
        canSteer: false,
        canQueue: false,
      }),
    ).toBe('send');
    expect(
      composerShortcutKind({
        blocked: false,
        sending: true,
        canSteer: true,
        canQueue: false,
      }),
    ).toBe('steer');
    expect(
      composerShortcutKind({
        blocked: false,
        sending: true,
        canSteer: false,
        canQueue: true,
      }),
    ).toBe('queue');
    expect(
      composerShortcutKind({
        blocked: true,
        sending: false,
        canSteer: false,
        canQueue: false,
      }),
    ).toBe('newline');
    expect(
      composerShortcutKind({
        blocked: false,
        sending: true,
        canSteer: false,
        canQueue: false,
      }),
    ).toBe('newline');
  });

  it('keeps Chinese shortcut copy short and honest', () => {
    expect(translate('zh', composerShortcutMessageKey('send'))).toBe(
      'Enter 发送 · Shift+Enter 换行',
    );
    expect(translate('zh', composerShortcutMessageKey('queue'))).toBe(
      'Enter 排队 · Shift+Enter 换行',
    );
    expect(translate('zh', composerShortcutMessageKey('steer'))).toBe(
      'Enter 补充 · Shift+Enter 换行',
    );
    expect(translate('zh', composerShortcutMessageKey('newline'))).toBe(
      'Shift+Enter 换行',
    );
    expect(translate('en', composerShortcutMessageKey('send'))).toBe(
      'Enter to send · Shift+Enter for a new line',
    );
    expect(translate('zh', composerSubmitMessageKey('send'))).toBe('发送');
    expect(translate('zh', composerSubmitMessageKey('queue'))).toBe('本轮结束后发送');
    expect(translate('zh', composerSubmitMessageKey('steer'))).toBe('补充');
    expect(translate('zh', composerStopMessageKey(false))).toBe('停止');
    expect(translate('zh', composerStopMessageKey(true))).toBe('正在停止');
    expect(translate('en', composerStopMessageKey(true))).toBe('Stopping');
  });
});

describe('queued follow-up visibility', () => {
  it('hides an empty queue and keeps count visible when queued', () => {
    expect(composerQueuedFollowUpView(null)).toBeNull();
    expect(composerQueuedFollowUpView('  ')).toBeNull();
    expect(composerQueuedFollowUpView('第二条；第三条', 2)).toEqual({
      count: 2,
      preview: '第二条；第三条',
    });
    expect(composerQueuedFollowUpView('只一条')).toEqual({
      count: 1,
      preview: '只一条',
    });
    expect(translate('zh', 'chat.composer.queuedCount', { count: 2 })).toBe('已排队 2 条');
    expect(translate('zh', 'chat.composer.queuedHint')).toBe('本轮结束后发送');
    expect(translate('en', 'chat.composer.queuedCount', { count: 2 })).toBe('2 queued');
  });
});

describe('composer focus after send', () => {
  it('returns focus to the input unless the box is disabled', () => {
    expect(composerShouldRestoreFocus({ textareaDisabled: false })).toBe(true);
    expect(composerShouldRestoreFocus({ textareaDisabled: true })).toBe(false);
  });
});
