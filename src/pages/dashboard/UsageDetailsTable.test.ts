import { describe, expect, it } from 'vitest';

import { buildPageItems } from './UsageDetailsTable';

describe('buildPageItems', () => {
  it('returns empty for zero pages', () => {
    expect(buildPageItems(1, 0)).toEqual([]);
  });

  it('lists all pages when total is small', () => {
    expect(buildPageItems(1, 5)).toEqual([1, 2, 3, 4, 5]);
    expect(buildPageItems(3, 7)).toEqual([1, 2, 3, 4, 5, 6, 7]);
  });

  it('inserts ellipsis for large page counts', () => {
    expect(buildPageItems(1, 12)).toEqual([1, 2, 'ellipsis', 12]);
    expect(buildPageItems(6, 12)).toEqual([1, 'ellipsis', 5, 6, 7, 'ellipsis', 12]);
    expect(buildPageItems(12, 12)).toEqual([1, 'ellipsis', 11, 12]);
  });
});
