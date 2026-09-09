import { describe, expect, it } from 'vitest';
import { translate } from '@/lib/i18n';
import {
  statusBarForwardDotClass,
  statusBarForwardKind,
  statusBarForwardMessageKey,
} from './status-bar-model';

describe('statusBarForwardKind', () => {
  it('treats a missing status as unavailable', () => {
    expect(statusBarForwardKind({ available: false, running: true })).toBe('unavailable');
  });

  it('prefers restarting over running', () => {
    expect(statusBarForwardKind({ available: true, running: true, restarting: true })).toBe(
      'restarting',
    );
  });

  it('maps running and stopped', () => {
    expect(statusBarForwardKind({ available: true, running: true, restarting: false })).toBe(
      'running',
    );
    expect(statusBarForwardKind({ available: true, running: false, restarting: false })).toBe(
      'stopped',
    );
  });
});

describe('statusBarForward copy keys', () => {
  it('uses chrome labels that do not collide with chat generating', () => {
    expect(statusBarForwardMessageKey('running')).toBe('chrome.localForwardRunning');
    expect(statusBarForwardMessageKey('stopped')).toBe('chrome.localForwardStopped');
    expect(statusBarForwardMessageKey('restarting')).toBe('chrome.localForwardRestarting');
    expect(statusBarForwardMessageKey('unavailable')).toBe('chrome.localForwardUnavailable');
    expect(translate('zh', 'chrome.localForwardRunning')).toBe('已开启');
    expect(translate('zh', 'chrome.localForwardRunning')).not.toBe('运行中');
    expect(translate('zh', 'chat.process.running')).toBe('生成中');
    expect(translate('zh', 'chat.toast.queuedAfterTurn')).toBe(
      translate('zh', 'chat.composer.sendAfterTurn'),
    );
  });

  it('does not use color alone: every kind still has a text key', () => {
    expect(statusBarForwardDotClass('running')).toContain('success');
    expect(statusBarForwardDotClass('restarting')).toContain('warning');
    expect(statusBarForwardDotClass('stopped')).toContain('muted');
  });
});
