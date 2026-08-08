import { describe, expect, it } from 'vitest';
import { isHttpUrl } from './open-external';

describe('isHttpUrl', () => {
  it('accepts http and https', () => {
    expect(isHttpUrl('https://skills.sh/foo')).toBe(true);
    expect(isHttpUrl('http://example.com')).toBe(true);
    expect(isHttpUrl('  HTTPS://X  ')).toBe(true);
  });

  it('rejects non-http schemes', () => {
    expect(isHttpUrl('file:///tmp/a')).toBe(false);
    expect(isHttpUrl('javascript:alert(1)')).toBe(false);
    expect(isHttpUrl('')).toBe(false);
    expect(isHttpUrl('/relative')).toBe(false);
  });
});
