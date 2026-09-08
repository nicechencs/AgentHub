import { describe, expect, it } from 'vitest';
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
  it('reuses existing runtime labels', () => {
    expect(statusBarForwardMessageKey('running')).toBe('routes.runtime.running');
    expect(statusBarForwardMessageKey('stopped')).toBe('routes.runtime.stopped');
    expect(statusBarForwardMessageKey('restarting')).toBe('routes.localForward.restarting');
    expect(statusBarForwardMessageKey('unavailable')).toBe('routes.runtime.unavailable');
  });

  it('does not use color alone: every kind still has a text key', () => {
    expect(statusBarForwardDotClass('running')).toContain('success');
    expect(statusBarForwardDotClass('restarting')).toContain('warning');
    expect(statusBarForwardDotClass('stopped')).toContain('muted');
  });
});
