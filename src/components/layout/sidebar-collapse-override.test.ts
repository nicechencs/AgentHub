import { describe, expect, it } from 'vitest';
import {
  effectiveCollapsed,
  onEnterRoutesArea,
  onExpandPrimaryFromRoutes,
  onLeaveRoutesArea,
  onToggleInRoutesArea,
  type SidebarCollapseMode,
} from './sidebar-collapse-override';

describe('sidebar-collapse-override', () => {
  it('uses session override when set, otherwise stored preference', () => {
    expect(effectiveCollapsed({ stored: false, session: null })).toBe(false);
    expect(effectiveCollapsed({ stored: true, session: null })).toBe(true);
    expect(effectiveCollapsed({ stored: false, session: true })).toBe(true);
    expect(effectiveCollapsed({ stored: true, session: false })).toBe(false);
  });

  it('auto-collapses on enter when no session override yet', () => {
    const next = onEnterRoutesArea({ stored: false, session: null });
    expect(next).toEqual({ stored: false, session: true });
  });

  it('does not reset an existing session override on re-enter', () => {
    const expanded: SidebarCollapseMode = { stored: true, session: false };
    expect(onEnterRoutesArea(expanded)).toEqual(expanded);
  });

  it('expand-from-routes forces session expanded', () => {
    expect(onExpandPrimaryFromRoutes({ stored: true, session: true })).toEqual({
      stored: true,
      session: false,
    });
  });

  it('toggle in routes flips session only', () => {
    expect(onToggleInRoutesArea({ stored: false, session: true })).toEqual({
      stored: false,
      session: false,
    });
    expect(onToggleInRoutesArea({ stored: true, session: null })).toEqual({
      stored: true,
      session: false,
    });
  });

  it('leave clears session override', () => {
    expect(onLeaveRoutesArea({ stored: false, session: true })).toEqual({
      stored: false,
      session: null,
    });
  });
});
