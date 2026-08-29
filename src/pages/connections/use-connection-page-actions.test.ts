import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { createTranslator } from '@/lib/i18n';
import {
  describeProviderSwitchError,
  SWITCH_WROTE_LIVE,
  switchErrorText,
  switchWroteLiveLabel,
} from './use-connection-page-actions';

const tZh = createTranslator('zh');
const tEn = createTranslator('en');

describe('describeProviderSwitchError', () => {
  it('maps Cursor unsupported / rollback to a localized live-write failure (zh)', () => {
    expect(describeProviderSwitchError(
      'cursor',
      'provider switch failed [unsupported]; compensation status: live=unsupported, database=ok [provider.switch.rollback]',
      tZh,
    )).toBe(
      '未能写入本机配置。Cursor 暂时不能把这份登录写到本机配置。请用 Cursor 自己的登录，或设置 CURSOR_API_KEY。',
    );
    expect(describeProviderSwitchError('cursor', new Error('unsupported'), tZh)).toContain('CURSOR_API_KEY');
    expect(describeProviderSwitchError('cursor', { message: 'unsupported' }, tZh)).toContain('未能写入本机配置');
  });

  it('maps Cursor unsupported / rollback to a localized live-write failure (en)', () => {
    expect(describeProviderSwitchError(
      'cursor',
      'provider switch failed [unsupported]; compensation status: live=unsupported, database=ok [provider.switch.rollback]',
      tEn,
    )).toBe(
      "Failed to write local config. Cursor can't write this login to its local config yet. Use Cursor's own sign-in, or set CURSOR_API_KEY.",
    );
    expect(describeProviderSwitchError('cursor', new Error('unsupported'), tEn)).toContain('CURSOR_API_KEY');
    expect(describeProviderSwitchError('cursor', { message: 'unsupported' }, tEn)).toContain('Failed to write local config');
  });

  it('falls back to the Chinese default when no translator is passed (backward compat)', () => {
    expect(describeProviderSwitchError(
      'cursor',
      'provider switch failed [unsupported]; compensation status: live=unsupported, database=ok [provider.switch.rollback]',
    )).toBe(
      '未能写入本机配置。Cursor 暂时不能把这份登录写到本机配置。请用 Cursor 自己的登录，或设置 CURSOR_API_KEY。',
    );
  });

  it('does not swallow a non-unsupported Cursor failure', () => {
    expect(describeProviderSwitchError('cursor', new Error('provider not found: missing'), tZh))
      .toBe('provider not found: missing');
    expect(describeProviderSwitchError('cursor', new Error('provider not found: missing'), tEn))
      .toBe('provider not found: missing');
  });

  it('keeps other agents\' error text', () => {
    expect(describeProviderSwitchError('claude', 'IO error: disk full [io]', tZh))
      .toBe('IO error: disk full');
    expect(describeProviderSwitchError('claude', 'IO error: disk full [io]', tEn))
      .toBe('IO error: disk full');
  });

  it('uses the localized failed-to-write fallback when the payload has no message', () => {
    expect(describeProviderSwitchError('claude', {}, tZh)).toBe('未能写入本机配置');
    expect(describeProviderSwitchError('claude', {}, tEn)).toBe('Failed to write local config');
    expect(describeProviderSwitchError('claude', {})).toBe('未能写入本机配置');
    expect(switchErrorText({})).toBe('');
  });
});

describe('switch toast copy', () => {
  it('names a successful live write (Chinese fallback constant, backward compat)', () => {
    expect(SWITCH_WROTE_LIVE).toBe('已写入本机配置');
  });

  it('switchWroteLiveLabel translates via t and falls back without one', () => {
    expect(switchWroteLiveLabel(tZh)).toBe('已写入本机配置');
    expect(switchWroteLiveLabel(tEn)).toBe('Wrote to local config');
    expect(switchWroteLiveLabel()).toBe('已写入本机配置');
    expect(switchWroteLiveLabel(tZh, 'catalogAppend')).toBe('已写入模型列表');
    expect(switchWroteLiveLabel(tEn, 'catalogAppend')).toBe('Wrote to the model list');
    expect(switchWroteLiveLabel(undefined, 'catalogAppend')).toBe('已写入模型列表');
  });

  it('keeps the wrote-live label off the bind-to-route success path', () => {
    const src = readFileSync(new URL('./use-connection-page-actions.ts', import.meta.url), 'utf8');
    expect(src).toContain('switchWroteLiveLabel(t, resolveAgentMeta(ticket.agentId).occupancy)');
    expect(src).toMatch(/const wroteLocal =\s*ticket\.agentId === targetAgent/);
  });
});

describe('guiErrorCode', () => {
  it('reads a trailing bracket code', async () => {
    const { guiErrorCode } = await import('@/lib/api/settings');
    expect(guiErrorCode('provider switch failed [provider.switch.rollback]')).toBe('provider.switch.rollback');
    expect(guiErrorCode(new Error('io failed [io]'))).toBe('io');
    expect(guiErrorCode('plain text')).toBeUndefined();
  });
});
