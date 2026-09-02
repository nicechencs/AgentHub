import { describe, expect, it } from 'vitest';
import {
  localForwardRestartBannerVisible,
  localForwardStartingComingBack,
} from './local-forward-restart';

describe('localForwardRestartBannerVisible', () => {
  it('shows on restarting status, event, or starting-coming-back', () => {
    expect(localForwardRestartBannerVisible({ restarting: true })).toBe(true);
    expect(localForwardRestartBannerVisible({ phase: 'restarting' })).toBe(true);
    expect(localForwardRestartBannerVisible({ startingComingBack: true })).toBe(true);
  });

  it('clears when ready unless status still says restarting or coming back', () => {
    expect(localForwardRestartBannerVisible({ phase: 'ready' })).toBe(false);
    expect(localForwardRestartBannerVisible({
      phase: 'ready',
      restarting: false,
      startingComingBack: false,
    })).toBe(false);
    expect(localForwardRestartBannerVisible({ phase: 'ready', restarting: true })).toBe(true);
    expect(localForwardRestartBannerVisible({
      phase: 'ready',
      startingComingBack: true,
    })).toBe(true);
  });

  it('stays hidden for a user stop', () => {
    expect(localForwardRestartBannerVisible({
      restarting: false,
      phase: null,
      startingComingBack: false,
    })).toBe(false);
  });
});

describe('localForwardStartingComingBack', () => {
  it('is true while restarting or starting and not yet running', () => {
    expect(localForwardStartingComingBack({ restarting: true, running: false })).toBe(true);
    expect(localForwardStartingComingBack({
      running: false,
      statuses: [{ state: 'starting' }],
    })).toBe(true);
  });

  it('is false when already running or only stopped', () => {
    expect(localForwardStartingComingBack({
      running: true,
      restarting: true,
      statuses: [{ state: 'starting' }],
    })).toBe(false);
    expect(localForwardStartingComingBack({
      running: false,
      statuses: [{ state: 'stopped' }],
    })).toBe(false);
    expect(localForwardStartingComingBack({ running: false, statuses: [] })).toBe(false);
  });
});
