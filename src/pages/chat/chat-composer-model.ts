/**
 * Chat 输入区：Enter / 排队 / 停止 / 焦点。纯函数，不碰 React。
 * 生成中没有真实补充或排队通道时，不画出可点的发送。
 */
import type { MessageKey } from '@/lib/i18n';

export type ComposerSubmitAction = 'send' | 'steer' | 'queue';
export type ComposerShortcutKind = 'send' | 'steer' | 'queue' | 'newline';

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

/** 空闲时保留发送按钮（无字则禁用）。生成中没有可执行动作时不画发送。 */
export function composerShowsSubmitButton(input: {
  sending: boolean;
  action: ComposerSubmitAction | null;
}): boolean {
  if (!input.sending) return true;
  return input.action != null;
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
