/**
 * Chat 输入区：Enter / 排队 / 停止 / 焦点。纯函数，不碰 React。
 * 右下角只留一个主按钮：生成中有字则发送/排队，清空则停止。
 */
import type { MessageKey } from '@/lib/i18n';

export type ComposerSubmitAction = 'send' | 'steer' | 'queue';
export type ComposerShortcutKind = 'send' | 'steer' | 'queue' | 'newline';
export type ComposerFooterControl = 'send' | 'stop';

/** Enter 发送；Shift+Enter 换行。中文输入法确认时不发送。 */
export function composerEnterShouldSubmit(input: {
  key: string;
  shiftKey: boolean;
  composing?: boolean;
  keyCode?: number;
}): boolean {
  if (input.key !== 'Enter' || input.shiftKey) return false;
  if (input.composing) return false;
  if (input.keyCode === 229) return false;
  return true;
}

export function composerPrimaryAction(input: {
  hasDraft: boolean;
  blocked: boolean;
  sending: boolean;
  canSteer: boolean;
  canQueue: boolean;
}): ComposerSubmitAction | null {
  if (!input.hasDraft || input.blocked) return null;
  if (!input.sending) return 'send';
  if (input.canSteer) return 'steer';
  if (input.canQueue) return 'queue';
  return null;
}

/**
 * 右下角只留一个主按钮：空闲一律发送；生成中有可执行动作（补充/排队）则发送，
 * 否则（空草稿或没有通道）同一位置改成停止。禁止并排。
 */
export function composerFooterControl(input: {
  sending: boolean;
  action: ComposerSubmitAction | null;
}): ComposerFooterControl {
  if (!input.sending) return 'send';
  if (input.action != null) return 'send';
  return 'stop';
}

/** 空闲时保留发送按钮（无字则禁用）。生成中没有可执行动作时改成停止。 */
export function composerShowsSubmitButton(input: {
  sending: boolean;
  action: ComposerSubmitAction | null;
}): boolean {
  return composerFooterControl(input) === 'send';
}

export function composerShortcutKind(input: {
  blocked: boolean;
  sending: boolean;
  canSteer: boolean;
  canQueue: boolean;
}): ComposerShortcutKind {
  if (input.blocked) return 'newline';
  if (input.sending && input.canSteer) return 'steer';
  if (input.sending && input.canQueue) return 'queue';
  if (input.sending) return 'newline';
  return 'send';
}

export function composerShortcutMessageKey(kind: ComposerShortcutKind): MessageKey {
  if (kind === 'steer') return 'chat.composer.shortcutSteer';
  if (kind === 'queue') return 'chat.composer.shortcutQueue';
  if (kind === 'newline') return 'chat.composer.shortcutNewline';
  return 'chat.composer.shortcutSend';
}

export function composerSubmitMessageKey(action: ComposerSubmitAction | null): MessageKey {
  if (action === 'steer') return 'chat.composer.add';
  if (action === 'queue') return 'chat.composer.sendAfterTurn';
  return 'chat.composer.send';
}

export function composerStopMessageKey(canceling: boolean): MessageKey {
  return canceling ? 'chat.composer.stopping' : 'chat.composer.stop';
}

/** Stop hover names Esc; cancelling keeps 正在停止 only. */
export function composerStopTitle(input: { canceling: boolean; stopLabel: string }): string {
  return input.canceling ? input.stopLabel : `${input.stopLabel} · Esc`;
}

export type ComposerCancelResult = 'pending' | 'requested' | 'none';

/** Keep 正在停止 until the turn ends. A miss (`none`) must not lock the button. */
export function composerKeepsStoppingAfterCancel(result: ComposerCancelResult): boolean {
  return result === 'pending' || result === 'requested';
}

/** Local click or a live runtime already in cancelling. */
export function composerCancelingVisible(input: {
  localCanceling: boolean;
  runtimePhase?: string | null;
}): boolean {
  return input.localCanceling || input.runtimePhase === 'cancelling';
}

export function composerQueuedFollowUpView(
  items: readonly { id: string; text: string }[] | null | undefined,
): { count: number; items: { id: string; text: string }[] } | null {
  const next = (items ?? [])
    .map((item) => ({ id: item.id, text: item.text.trim() }))
    .filter((item) => item.id && item.text);
  if (next.length === 0) return null;
  return { count: next.length, items: next };
}

export function composerShouldRestoreFocus(input: { textareaDisabled: boolean }): boolean {
  return !input.textareaDisabled;
}

/** Prefer the live textarea so rapid typing + Enter does not send a stale React draft. */
export function composerLiveSendText(input: {
  textareaValue?: string | null;
  draft: string;
}): string {
  return input.textareaValue ?? input.draft;
}

/** Hold the just-sent payload long enough for late key/IME events to drain. */
export const COMPOSER_SEND_SETTLE_MS = 400;

/** Growing tail leftovers shorter than this still match as residual during settle. */
export const COMPOSER_GROWING_RESIDUAL_MIN = 8;

/** Exact / prefix / suffix leftovers of the submitted payload — never a new follow-up. */
export function composerIsResidualOfSent(input: { text: string; sent: string }): boolean {
  const text = input.text.trim();
  const sent = input.sent.trim();
  if (!text) return true;
  if (!sent) return false;
  if (text === sent || sent.endsWith(text) || sent.startsWith(text)) return true;
  // "doing any" while sent ends with "doing any work." — same burst, not a new line.
  if (text.length >= COMPOSER_GROWING_RESIDUAL_MIN && sent.includes(text)) return true;
  return false;
}

/**
 * Queue only a real next line. A leftover fragment of `lastSent` must not enqueue.
 */
export function composerQueueableFollowUpText(input: {
  text: string;
  lastSent?: string | null;
  settling?: boolean;
}): string | null {
  const text = input.text.trim();
  if (!text) return null;
  if (input.lastSent && composerIsResidualOfSent({ text, sent: input.lastSent })) return null;
  if (input.settling && input.lastSent?.includes(text)) return null;
  return text;
}

/** Parent put the just-sent prompt back (stop / failure). That is not leftover. */
export function composerShouldKeepRestoredSent(input: {
  draft: string;
  sent: string;
}): boolean {
  const draft = input.draft.trim();
  const sent = input.sent.trim();
  return Boolean(sent) && draft === sent;
}

/**
 * After send: drop residual / same-burst leftovers; absorb a late completion of
 * the submitted payload into the lock; keep a real follow-up that is not in `sent`.
 */
export function composerShouldHoldSendLock(input: {
  now: number;
  settleUntil: number;
  next: string;
  sent: string;
}): { hold: boolean; sent: string; draft: string } {
  const sent = input.sent.trim();
  const nextTrim = input.next.trim();
  const inSettle = input.now < input.settleUntil;
  if (!nextTrim) return { hold: Boolean(sent), sent, draft: '' };
  if (!sent) return { hold: false, sent: '', draft: input.next };
  if (composerIsResidualOfSent({ text: nextTrim, sent })) {
    return { hold: true, sent, draft: '' };
  }
  if (nextTrim.startsWith(sent)) {
    return { hold: true, sent: nextTrim, draft: '' };
  }
  if (inSettle && sent.includes(nextTrim)) {
    return { hold: true, sent, draft: '' };
  }
  return { hold: false, sent: '', draft: input.next };
}

/**
 * After a successful send, drop leftover draft that is still the sent prompt
 * (or a prefix / suffix of it). Keep only text typed after send.
 */
export function composerDraftAfterSuccessfulSend(input: {
  draft: string;
  sent: string;
}): string {
  const draft = input.draft;
  const sent = input.sent;
  if (!draft.trim()) return '';
  if (!sent.trim()) return draft;
  if (composerIsResidualOfSent({ text: draft, sent })) return '';
  const draftTrim = draft.trim();
  const sentTrim = sent.trim();
  if (draftTrim.startsWith(sentTrim)) {
    const idx = draft.indexOf(sentTrim);
    return idx >= 0 ? draft.slice(idx + sentTrim.length) : '';
  }
  return draft;
}

/**
 * Stop / cancel: keep what the user already typed, else the queued line,
 * else the prompt that was just sent so they can resend.
 */
export function composerDraftAfterCancel(input: {
  draft: string;
  queuedDraft?: string;
  lastSent?: string;
}): string {
  if (input.draft.trim()) return input.draft;
  const queued = input.queuedDraft?.trim() ?? '';
  if (queued) return input.queuedDraft ?? queued;
  return input.lastSent ?? '';
}
