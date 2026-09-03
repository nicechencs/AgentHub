/** Shared Routes product-surface helpers. Pages must not own these constants. */

export const ROUTES_NAV_LABEL = 'Routes';
/** Routes area root. Sidebar "路由" lands here, then redirects to the board. */
export const ROUTES_PATH = '/routes';
export const ROUTES_BOARD_PATH = `${ROUTES_PATH}/board`;
export const ROUTES_POOL_PATH = `${ROUTES_PATH}/pool`;
export const ROUTES_TOKENS_PATH = `${ROUTES_PATH}/tokens`;
export const ROUTES_ACTIVITY_PATH = `${ROUTES_PATH}/activity`;

function routesSearchParams(search: string): URLSearchParams {
  const params = new URLSearchParams(search.startsWith('?') ? search.slice(1) : search);
  params.delete('tab');
  return params;
}

/**
 * `/routes` and old `/adapter` `/router` `/bridges` bookmarks.
 * Bare entry → board. `?profile=` → auth-pool detail.
 */
export function routesIndexRedirectTo(search: string): string {
  const params = routesSearchParams(search);
  const profile = params.get('profile');
  if (profile) {
    params.delete('profile');
    const rest = params.toString();
    const href = `${ROUTES_POOL_PATH}?profile=${encodeURIComponent(profile)}`;
    return rest ? `${href}&${rest}` : href;
  }
  const qs = params.toString();
  return qs ? `${ROUTES_BOARD_PATH}?${qs}` : ROUTES_BOARD_PATH;
}

/** URL compat for old `/bridges` `/adapter` `/router` deep links. */
export function legacyBridgesRedirectTo(search: string): string {
  return routesIndexRedirectTo(search);
}

export function routesHrefForProfile(profileId: string | null | undefined): string {
  return profileId
    ? `${ROUTES_POOL_PATH}?profile=${encodeURIComponent(profileId)}`
    : ROUTES_POOL_PATH;
}
