import { describe, expect, it } from 'vitest';
import { flattenKeys } from '../index';
import { en } from './en';
import { zh } from './zh';

describe('locale key parity', () => {
  it('zh and en expose the same leaf keys', () => {
    const zhKeys = flattenKeys(zh).sort();
    const enKeys = flattenKeys(en).sort();
    expect(enKeys).toEqual(zhKeys);
    expect(zhKeys.length).toBeGreaterThan(50);
  });
});
