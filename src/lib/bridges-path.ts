/** Shared local-routes helpers. Pages must not own these constants. */

export const BRIDGES_NAV_LABEL = 'Routes';
export const BRIDGES_PATH = '/routes';

/** Drop leftover `?tab=` from `/adapter` / `/router` / `/bridges` bookmarks. */
export function legacyBridgesRedirectTo(search: string): string {
  const params = new URLSearchParams(search.startsWith('?') ? search.slice(1) : search);
  params.delete('tab');
  const qs = params.toString();
  return qs ? `${BRIDGES_PATH}?${qs}` : BRIDGES_PATH;
}

export function bridgesHrefForProfile(profileId: string | null | undefined): string {
  return profileId ? `${BRIDGES_PATH}?profile=${encodeURIComponent(profileId)}` : BRIDGES_PATH;
}
