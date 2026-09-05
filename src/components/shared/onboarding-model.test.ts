import { describe, expect, it } from 'vitest';
import {
  EMPTY_ONBOARDING_USAGE,
  hasOnboardingUsageChoice,
  navVisibilityForUsage,
  toggleOnboardingUsage,
} from './onboarding-model';

describe('onboarding usage choice', () => {
  it('starts with neither page selected', () => {
    expect(EMPTY_ONBOARDING_USAGE).toEqual({ routes: false, sub2api: false });
    expect(hasOnboardingUsageChoice(EMPTY_ONBOARDING_USAGE)).toBe(false);
  });

  it('toggles each page independently', () => {
    const routesOnly = toggleOnboardingUsage(EMPTY_ONBOARDING_USAGE, 'routes');
    expect(routesOnly).toEqual({ routes: true, sub2api: false });
    expect(hasOnboardingUsageChoice(routesOnly)).toBe(true);

    const both = toggleOnboardingUsage(routesOnly, 'sub2api');
    expect(both).toEqual({ routes: true, sub2api: true });

    const sub2apiOnly = toggleOnboardingUsage(both, 'routes');
    expect(sub2apiOnly).toEqual({ routes: false, sub2api: true });
  });

  it('hides Sub2API when only local routing is chosen', () => {
    expect(navVisibilityForUsage({ routes: true, sub2api: false })).toEqual({
      routesNavVisible: true,
      sub2apiNavVisible: false,
    });
  });

  it('hides Routes when only Sub2API is chosen', () => {
    expect(navVisibilityForUsage({ routes: false, sub2api: true })).toEqual({
      routesNavVisible: false,
      sub2apiNavVisible: true,
    });
  });

  it('keeps both sidebar entries when both are chosen', () => {
    expect(navVisibilityForUsage({ routes: true, sub2api: true })).toEqual({
      routesNavVisible: true,
      sub2apiNavVisible: true,
    });
  });
});
