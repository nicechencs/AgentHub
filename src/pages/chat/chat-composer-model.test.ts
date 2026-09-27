import { describe, expect, it } from 'vitest';
import { translate } from '@/lib/i18n';
import {
  composerCancelingVisible,
  composerDraftAfterCancel,
  composerDraftAfterSteerAck,
  COMPOSER_SEND_SETTLE_MS,
  composerDraftAfterSuccessfulSend,
  composerEnterShouldSubmit,
  composerIsResidualOfSent,
  composerLiveSendText,
  composerQueueableFollowUpText,
  composerShouldHoldSendLock,
  composerShouldKeepRestoredSent,
  composerFooterControl,
  composerKeepsStoppingAfterCancel,
  composerPrimaryAction,
  composerQueuedFollowUpView,
  composerShortcutKind,
  composerShortcutMessageKey,
  composerShouldRestoreFocus,
  composerShowsSubmitButton,
  composerStopMessageKey,
  composerStopTitle,
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

describe('composer footer control', () => {
  it('shows Send when idle, even if the draft is empty', () => {
    expect(composerFooterControl({ sending: false, action: null })).toBe('send');
    expect(composerFooterControl({ sending: false, action: 'send' })).toBe('send');
  });

  it('shows Send while generating only when there is a real next action', () => {
    expect(composerFooterControl({ sending: true, action: 'queue' })).toBe('send');
    expect(composerFooterControl({ sending: true, action: 'steer' })).toBe('send');
  });

  it('shows Stop in the same slot when generating and the composer is empty or blocked', () => {
    expect(composerFooterControl({ sending: true, action: null })).toBe('stop');
  });

  it('names Esc on Stop until cancelling', () => {
    expect(composerStopTitle({ canceling: false, stopLabel: '停止' })).toBe('停止 · Esc');
    expect(composerStopTitle({ canceling: false, stopLabel: 'Stop' })).toBe('Stop · Esc');
    expect(composerStopTitle({ canceling: true, stopLabel: '正在停止' })).toBe('正在停止');
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

  it('keeps 正在停止 after a real cancel request until the turn ends', () => {
    expect(composerKeepsStoppingAfterCancel('requested')).toBe(true);
    expect(composerKeepsStoppingAfterCancel('pending')).toBe(true);
    expect(composerKeepsStoppingAfterCancel('none')).toBe(false);
    expect(composerCancelingVisible({ localCanceling: true, runtimePhase: 'running' })).toBe(true);
    expect(composerCancelingVisible({ localCanceling: false, runtimePhase: 'cancelling' })).toBe(
      true,
    );
    expect(composerCancelingVisible({ localCanceling: false, runtimePhase: 'running' })).toBe(
      false,
    );
  });
});

describe('queued follow-up visibility', () => {
  it('hides an empty queue and keeps each line as its own item', () => {
    expect(composerQueuedFollowUpView(null)).toBeNull();
    expect(composerQueuedFollowUpView([])).toBeNull();
    expect(composerQueuedFollowUpView([{ id: 'blank', text: '  ' }])).toBeNull();
    expect(
      composerQueuedFollowUpView([
        { id: 'q-2', text: '第二条' },
        { id: 'q-3', text: '第三条' },
      ]),
    ).toEqual({
      count: 2,
      items: [
        { id: 'q-2', text: '第二条' },
        { id: 'q-3', text: '第三条' },
      ],
    });
    expect(composerQueuedFollowUpView([{ id: 'q-1', text: '只一条' }])).toEqual({
      count: 1,
      items: [{ id: 'q-1', text: '只一条' }],
    });
    expect(translate('zh', 'chat.toast.followUpKept')).toBe('这条和刚发出的重复，还留在输入框');
    expect(translate('en', 'chat.toast.followUpKept')).toBe(
      'This matches what you just sent. It stays in the box.',
    );
    expect(translate('zh', 'chat.composer.queuedCount', { count: 2 })).toBe('已排队 2 条');
    expect(translate('zh', 'chat.composer.queuedHint')).toBe('本轮结束后发送');
    expect(translate('zh', 'chat.composer.cancelQueuedItem')).toBe('取消这条');
    expect(translate('zh', 'chat.composer.cancelAllQueued')).toBe('全部取消');
    expect(translate('en', 'chat.composer.queuedCount', { count: 2 })).toBe('2 queued');
    expect(translate('en', 'chat.composer.cancelQueuedItem')).toBe('Remove this');
    expect(translate('en', 'chat.composer.cancelAllQueued')).toBe('Cancel all');
  });
});

describe('composer clear-on-send', () => {
  it('prefers the live textarea over a stale React draft', () => {
    expect(
      composerLiveSendText({
        textareaValue: 'Write a plan and then summarize. Do not edit files.',
        draft: 'Write a plan and then ',
      }),
    ).toBe('Write a plan and then summarize. Do not edit files.');
    expect(composerLiveSendText({ draft: 'hello' })).toBe('hello');
  });

  it('drops a sent prompt leftover, including a residual suffix', () => {
    const sent = 'Write a 3-step plan and then summarize. Do not edit files.';
    expect(composerDraftAfterSuccessfulSend({ draft: sent, sent })).toBe('');
    expect(
      composerDraftAfterSuccessfulSend({ draft: 'summarize. Do not edit files.', sent }),
    ).toBe('');
    expect(composerDraftAfterSuccessfulSend({ draft: 'Write a 3-step plan', sent })).toBe('');
    expect(composerDraftAfterSuccessfulSend({ draft: '  ', sent })).toBe('');
  });

  it('keeps a stop-restored sent prompt and does not treat it as leftover', () => {
    expect(
      composerShouldKeepRestoredSent({
        draft: 'e2e mock stop',
        sent: 'e2e mock stop',
      }),
    ).toBe(true);
    expect(
      composerShouldKeepRestoredSent({
        draft: '',
        sent: 'e2e mock stop',
      }),
    ).toBe(false);
    expect(
      composerQueueableFollowUpText({
        text: 'e2e mock stop',
        lastSent: 'e2e mock stop',
        settling: true,
      }),
    ).toBeNull();
    expect(
      composerQueueableFollowUpText({
        text: 'e2e mock stop',
        lastSent: 'e2e mock stop',
      }),
    ).toBe('e2e mock stop');
  });

  it('restores the sent prompt on stop when the box is empty', () => {
    expect(
      composerDraftAfterCancel({
        draft: '',
        queuedDraft: '',
        lastSent: 'e2e mock stop',
      }),
    ).toBe('e2e mock stop');
    expect(
      composerDraftAfterCancel({
        draft: 'already typing',
        queuedDraft: 'queued',
        lastSent: 'e2e mock stop',
      }),
    ).toBe('already typing');
    expect(
      composerDraftAfterCancel({
        draft: '',
        queuedDraft: 'queued line',
        lastSent: 'e2e mock stop',
      }),
    ).toBe('queued line');
  });

  it('keeps text typed after a successful send', () => {
    expect(
      composerDraftAfterSuccessfulSend({
        draft: 'first prompt follow-up',
        sent: 'first prompt',
      }),
    ).toBe(' follow-up');
    expect(
      composerDraftAfterSuccessfulSend({
        draft: 'another question',
        sent: 'first prompt',
      }),
    ).toBe('another question');
  });

  it('does not treat a trailing fragment of the just-sent prompt as queueable', () => {
    const sent = "I'll write a short 3-step UI retest plan and show it before doing any work.";
    const leftover = 'doing any work.';
    expect(composerIsResidualOfSent({ text: leftover, sent })).toBe(true);
    expect(composerIsResidualOfSent({ text: sent, sent })).toBe(true);
    expect(composerIsResidualOfSent({ text: "I'll write a short 3-step", sent })).toBe(true);
    expect(composerIsResidualOfSent({ text: 'doing any', sent })).toBe(true);
    expect(composerIsResidualOfSent({ text: '下一句', sent })).toBe(false);
    expect(composerQueueableFollowUpText({ text: leftover, lastSent: sent, settling: true })).toBeNull();
    expect(composerQueueableFollowUpText({ text: sent, lastSent: sent, settling: true })).toBeNull();
    expect(composerQueueableFollowUpText({ text: leftover, lastSent: sent })).toBe(leftover);
    expect(composerQueueableFollowUpText({ text: sent, lastSent: sent })).toBe(sent);
    expect(composerQueueableFollowUpText({ text: '  ', lastSent: sent })).toBeNull();
    expect(
      composerQueueableFollowUpText({
        text: 'please inspect the preview header next',
        lastSent: sent,
      }),
    ).toBe('please inspect the preview header next');
  });

  it('holds late controlled-input leftovers in the same send burst', () => {
    const sent = "I'll write a short 3-step UI retest plan and show it before doing any work.";
    const prefix = "I'll write a short 3-step UI retest plan and show it before ";
    expect(
      composerShouldHoldSendLock({
        now: 10,
        settleUntil: COMPOSER_SEND_SETTLE_MS,
        next: 'doing any work.',
        sent,
      }),
    ).toEqual({ hold: true, sent, draft: '' });
    expect(
      composerShouldHoldSendLock({
        now: 10,
        settleUntil: COMPOSER_SEND_SETTLE_MS,
        next: prefix + 'doing any work.',
        sent: prefix,
      }),
    ).toEqual({ hold: true, sent: (prefix + 'doing any work.').trim(), draft: '' });
    expect(
      composerShouldHoldSendLock({
        now: COMPOSER_SEND_SETTLE_MS + 50,
        settleUntil: COMPOSER_SEND_SETTLE_MS,
        next: 'doing any work.',
        sent,
      }),
    ).toEqual({ hold: false, sent: '', draft: 'doing any work.' });
    expect(
      composerShouldHoldSendLock({
        now: COMPOSER_SEND_SETTLE_MS + 50,
        settleUntil: COMPOSER_SEND_SETTLE_MS,
        next: sent,
        sent,
      }),
    ).toEqual({ hold: false, sent: '', draft: sent });
    expect(
      composerShouldHoldSendLock({
        now: COMPOSER_SEND_SETTLE_MS + 50,
        settleUntil: COMPOSER_SEND_SETTLE_MS,
        next: 'P',
        sent: 'Please inspect the preview header',
      }),
    ).toEqual({ hold: false, sent: '', draft: 'P' });
    expect(
      composerShouldHoldSendLock({
        now: 10,
        settleUntil: COMPOSER_SEND_SETTLE_MS,
        next: '下一句',
        sent,
      }),
    ).toEqual({ hold: false, sent: '', draft: '下一句' });
    expect(
      composerShouldHoldSendLock({
        now: COMPOSER_SEND_SETTLE_MS + 50,
        settleUntil: COMPOSER_SEND_SETTLE_MS,
        next: 'please inspect the preview header next',
        sent,
      }),
    ).toEqual({
      hold: false,
      sent: '',
      draft: 'please inspect the preview header next',
    });
    expect(composerQueueableFollowUpText({
      text: 'doing',
      lastSent: sent,
      settling: true,
    })).toBeNull();
    expect(composerQueueableFollowUpText({
      text: '下一句',
      lastSent: sent,
      settling: true,
    })).toBe('下一句');
  });
});

describe('composer steer ack draft', () => {
  it('clears the steered line after ack and restores it on reject', () => {
    expect(
      composerDraftAfterSteerAck({
        ok: true,
        draft: 'add a log line',
        steered: 'add a log line',
      }),
    ).toBe('');
    expect(
      composerDraftAfterSteerAck({
        ok: false,
        draft: '',
        steered: 'add a log line',
      }),
    ).toBe('add a log line');
    expect(
      composerDraftAfterSteerAck({
        ok: false,
        draft: 'typed more',
        steered: 'add a log line',
      }),
    ).toBe('typed more');
    expect(
      composerDraftAfterSteerAck({
        ok: true,
        draft: 'typed more after steer',
        steered: 'add a log line',
      }),
    ).toBe('typed more after steer');
  });
});

describe('composer focus after send', () => {
  it('returns focus to the input unless the box is disabled', () => {
    expect(composerShouldRestoreFocus({ textareaDisabled: false })).toBe(true);
    expect(composerShouldRestoreFocus({ textareaDisabled: true })).toBe(false);
  });
});
