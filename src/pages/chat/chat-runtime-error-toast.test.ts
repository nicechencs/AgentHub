import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { createTranslator } from '@/lib/i18n';
import { localizeChatFailure } from './chat-format';
import {
  RECOVER_ACTIVE_INTERRUPT_MESSAGE,
  isRuntimeInterruptErrorMessage,
  shouldToastRuntimeError,
  snapshotErrorToastSource,
} from './chat-runtime-error-toast';

const sendSource = readFileSync(
  path.join(path.dirname(fileURLToPath(import.meta.url)), 'use-chat-page-send.ts'),
  'utf8',
);

describe('runtime error toast', () => {
  it('does not toast a historical error, including recover_active interrupt', () => {
    expect(snapshotErrorToastSource(false)).toBe('historical');
    expect(
      shouldToastRuntimeError({
        source: 'historical',
        message: RECOVER_ACTIVE_INTERRUPT_MESSAGE,
      }),
    ).toBe(false);
    expect(
      shouldToastRuntimeError({
        source: 'historical',
        message: 'model unavailable',
      }),
    ).toBe(false);
  });

  it('toasts a live non-interrupt error, but not recover_active interrupt copy', () => {
    expect(snapshotErrorToastSource(true)).toBe('live');
    expect(
      shouldToastRuntimeError({
        source: 'live',
        message: 'model unavailable',
      }),
    ).toBe(true);
    expect(
      shouldToastRuntimeError({
        source: 'live',
        message: RECOVER_ACTIVE_INTERRUPT_MESSAGE,
      }),
    ).toBe(false);
    expect(
      shouldToastRuntimeError({
        source: 'live',
        message: 'runtime interrupted',
      }),
    ).toBe(false);
    expect(
      shouldToastRuntimeError({
        source: 'live',
        message: '   ',
      }),
    ).toBe(false);
  });

  it('localizes recover_active interrupt copy instead of leaving the raw English string', () => {
    expect(isRuntimeInterruptErrorMessage(RECOVER_ACTIVE_INTERRUPT_MESSAGE)).toBe(true);
    expect(localizeChatFailure(RECOVER_ACTIVE_INTERRUPT_MESSAGE, createTranslator('zh'))).toBe(
      '当前轮已中断。可以直接在这场对话里继续发送。',
    );
    expect(localizeChatFailure(RECOVER_ACTIVE_INTERRUPT_MESSAGE, createTranslator('en'))).toBe(
      'This turn was interrupted. You can keep sending in this chat.',
    );
  });

  it('wires snapshot replay through the toast gate instead of ev.message', () => {
    expect(sendSource).toContain('snapshotErrorToastSource');
    expect(sendSource).toContain('shouldToastRuntimeError');
    expect(sendSource).toContain('localizeChatFailure(ev.message, t)');
    expect(sendSource).not.toMatch(/toast\(\{\s*title:\s*ev\.message/);
  });
});
