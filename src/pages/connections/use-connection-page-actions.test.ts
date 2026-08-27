import { describe, expect, it } from 'vitest';
import {
  describeProviderSwitchError,
  SWITCH_WROTE_LIVE,
  switchErrorText,
} from './use-connection-page-actions';

describe('describeProviderSwitchError', () => {
  it('maps Cursor unsupported / rollback to a Chinese live-write failure', () => {
    expect(describeProviderSwitchError(
      'cursor',
      'provider switch failed [unsupported]; compensation status: live=unsupported, database=ok [provider.switch.rollback]',
    )).toBe(
      '未能写入本机配置。Cursor 暂时不能把这份登录写到本机配置。请用 Cursor 自己的登录，或设置 CURSOR_API_KEY。',
    );
    expect(describeProviderSwitchError('cursor', new Error('unsupported'))).toContain('CURSOR_API_KEY');
    expect(describeProviderSwitchError('cursor', { message: 'unsupported' })).toContain('未能写入本机配置');
  });

  it('does not swallow a non-unsupported Cursor failure', () => {
    expect(describeProviderSwitchError('cursor', new Error('provider not found: missing')))
      .toBe('provider not found: missing');
  });

  it('keeps other agents\' error text', () => {
    expect(describeProviderSwitchError(
      'claude',
      'IO error: disk full [io]',
    )).toBe('IO error: disk full');
  });

  it('uses 未能写入本机配置 when the payload has no message', () => {
    expect(describeProviderSwitchError('claude', {})).toBe('未能写入本机配置');
    expect(switchErrorText({})).toBe('');
  });
});

describe('switch toast copy', () => {
  it('names a successful live write', () => {
    expect(SWITCH_WROTE_LIVE).toBe('已写入本机配置');
  });
});
