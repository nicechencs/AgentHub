/**
 * @deprecated Compatibility re-export layer. Import from `@/lib/routes-path` instead.
 * Kept so external/doc links and any straggling imports keep working.
 */
export {
  ROUTES_NAV_LABEL,
  ROUTES_PATH,
  ROUTES_BOARD_PATH,
  ROUTES_POOL_PATH,
  ROUTES_TOKENS_PATH,
  ROUTES_ACTIVITY_PATH,
  routesIndexRedirectTo,
  routesHrefForProfile,
  legacyBridgesRedirectTo,
} from './routes-path';

import { ROUTES_NAV_LABEL, ROUTES_PATH, routesHrefForProfile } from './routes-path';

/** @deprecated Use {@link ROUTES_PATH} from `@/lib/routes-path`. */
export const BRIDGES_PATH = ROUTES_PATH;
/** @deprecated Use {@link ROUTES_NAV_LABEL} from `@/lib/routes-path`. */
export const BRIDGES_NAV_LABEL = ROUTES_NAV_LABEL;
/** @deprecated Use {@link routesHrefForProfile} from `@/lib/routes-path`. */
export const bridgesHrefForProfile = routesHrefForProfile;
