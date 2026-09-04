import { describe, expect, it } from 'vitest';
import {
  activityKeyOptionLabel,
  buildActivityPageItems,
  clampActivityPage,
  parseActivityEndpointParam,
  parseActivityPageParam,
  resolveActivityKeyQuery,
} from './activity-query-model';

describe('activity-query-model', () => {
  it('parses endpoint and page query params', () => {
    expect(parseActivityEndpointParam('messages')).toBe('messages');
    expect(parseActivityEndpointParam('responses_grok')).toBe('responses_grok');
    expect(parseActivityEndpointParam('nope')).toBeNull();
    expect(parseActivityPageParam('3')).toBe(3);
    expect(parseActivityPageParam('0')).toBe(1);
    expect(parseActivityPageParam('x')).toBe(1);
  });

  it('resolves a key filter from the selected token', () => {
    const tokens = [
      { id: 'pool-1', poolId: 'pool-1', token: 'ahb_secret1234', name: 'Claude', primary: true },
    ];
    expect(resolveActivityKeyQuery('pool-1', tokens)).toEqual({
      keyLast4: '1234',
      poolId: 'pool-1',
    });
    expect(resolveActivityKeyQuery('', tokens)).toBeNull();
    expect(activityKeyOptionLabel(tokens[0]!)).toBe('ahb_••••1234 Claude');
  });

  it('builds compact page items and clamps the current page', () => {
    expect(buildActivityPageItems(1, 3)).toEqual([1, 2, 3]);
    expect(buildActivityPageItems(4, 10)).toEqual([1, 'ellipsis', 3, 4, 5, 'ellipsis', 10]);
    expect(clampActivityPage(9, 50, 50)).toBe(1);
    expect(clampActivityPage(3, 120, 50)).toBe(3);
  });
});
