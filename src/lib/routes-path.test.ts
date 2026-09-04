import { describe, expect, it } from 'vitest';
import {
  ROUTES_BOARD_PATH,
  ROUTES_POOL_PATH,
  ROUTES_SUB2API_PATH,
  SUB2API_PATH,
  legacyBridgesRedirectTo,
  routesHrefForProfile,
  routesIndexRedirectTo,
} from './routes-path';

describe('routes-path', () => {
  it('builds a profile deep link onto the auth-pool workbench', () => {
    expect(routesHrefForProfile(null)).toBe(ROUTES_POOL_PATH);
    expect(routesHrefForProfile(undefined)).toBe(ROUTES_POOL_PATH);
    expect(routesHrefForProfile('p1')).toBe(`${ROUTES_POOL_PATH}?profile=p1`);
    expect(routesHrefForProfile('a b')).toBe(`${ROUTES_POOL_PATH}?profile=a%20b`);
  });

  it('sends /routes to the board, and ?profile= to the auth pool', () => {
    expect(routesIndexRedirectTo('')).toBe(ROUTES_BOARD_PATH);
    expect(routesIndexRedirectTo('?tab=old')).toBe(ROUTES_BOARD_PATH);
    expect(routesIndexRedirectTo('?profile=p1')).toBe(`${ROUTES_POOL_PATH}?profile=p1`);
    expect(routesIndexRedirectTo('?tab=profiles&profile=p1')).toBe(
      `${ROUTES_POOL_PATH}?profile=p1`,
    );
  });

  it('keeps Sub2API on the primary path and the old nested path for redirects', () => {
    expect(SUB2API_PATH).toBe('/sub2api');
    expect(ROUTES_SUB2API_PATH).toBe('/routes/sub2api');
  });

  it('drops leftover tab= from legacy adapter/router/bridges bookmarks', () => {
    expect(legacyBridgesRedirectTo('?tab=profiles&profile=p1')).toBe(
      `${ROUTES_POOL_PATH}?profile=p1`,
    );
    expect(legacyBridgesRedirectTo('tab=old')).toBe(ROUTES_BOARD_PATH);
    expect(legacyBridgesRedirectTo('')).toBe(ROUTES_BOARD_PATH);
  });
});
