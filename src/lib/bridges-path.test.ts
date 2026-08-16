import { describe, expect, it } from 'vitest';
import {
  BRIDGES_PATH,
  bridgesHrefForProfile,
  legacyBridgesRedirectTo,
} from './bridges-path';

describe('bridges-path', () => {
  it('builds a profile deep link and encodes the id', () => {
    expect(bridgesHrefForProfile(null)).toBe(BRIDGES_PATH);
    expect(bridgesHrefForProfile(undefined)).toBe(BRIDGES_PATH);
    expect(bridgesHrefForProfile('p1')).toBe(`${BRIDGES_PATH}?profile=p1`);
    expect(bridgesHrefForProfile('a b')).toBe(`${BRIDGES_PATH}?profile=a%20b`);
  });

  it('drops leftover tab= from legacy adapter/router/bridges bookmarks', () => {
    expect(legacyBridgesRedirectTo('?tab=profiles&profile=p1')).toBe(
      `${BRIDGES_PATH}?profile=p1`,
    );
    expect(legacyBridgesRedirectTo('tab=old')).toBe(BRIDGES_PATH);
    expect(legacyBridgesRedirectTo('')).toBe(BRIDGES_PATH);
  });
});
