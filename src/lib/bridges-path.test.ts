import { describe, expect, it } from 'vitest';
import { ROUTES_NAV_LABEL, ROUTES_PATH, routesHrefForProfile } from './routes-path';
import { BRIDGES_NAV_LABEL, BRIDGES_PATH, bridgesHrefForProfile } from './bridges-path';

describe('bridges-path (deprecated compat layer)', () => {
  it('re-exports the routes-path constants under the legacy names', () => {
    expect(BRIDGES_PATH).toBe(ROUTES_PATH);
    expect(BRIDGES_NAV_LABEL).toBe(ROUTES_NAV_LABEL);
    expect(bridgesHrefForProfile).toBe(routesHrefForProfile);
  });
});
