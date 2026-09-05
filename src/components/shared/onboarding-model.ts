/** First-run usage choice: which manage pages to keep in the sidebar. */

export const ONBOARDING_USAGE_IDS = ['routes', 'sub2api'] as const;

export type OnboardingUsageId = (typeof ONBOARDING_USAGE_IDS)[number];

export type OnboardingUsageSelection = Record<OnboardingUsageId, boolean>;

export const EMPTY_ONBOARDING_USAGE: OnboardingUsageSelection = {
  routes: false,
  sub2api: false,
};

export function toggleOnboardingUsage(
  current: OnboardingUsageSelection,
  id: OnboardingUsageId,
): OnboardingUsageSelection {
  return { ...current, [id]: !current[id] };
}

export function hasOnboardingUsageChoice(selection: OnboardingUsageSelection): boolean {
  return selection.routes || selection.sub2api;
}

/** Maps the first-run choice onto sidebar visibility. Unselected pages stay reachable from Settings. */
export function navVisibilityForUsage(selection: OnboardingUsageSelection): {
  routesNavVisible: boolean;
  sub2apiNavVisible: boolean;
} {
  return {
    routesNavVisible: selection.routes,
    sub2apiNavVisible: selection.sub2api,
  };
}
