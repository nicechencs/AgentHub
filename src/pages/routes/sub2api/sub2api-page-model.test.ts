import { describe, expect, it } from 'vitest';
import { sub2apiDisplayName, sub2apiPagePhase } from './sub2api-page-model';

describe('sub2api page model', () => {
  it('maps three page states: logged-out / logging-in / logged-in', () => {
    expect(sub2apiPagePhase(null, true)).toBe('logging-in');
    expect(
      sub2apiPagePhase(
        { siteUrl: 'https://x', gatewayBaseUrl: 'https://x', accessToken: 't' },
        false,
      ),
    ).toBe('logged-in');
    expect(sub2apiPagePhase(null, false)).toBe('logged-out');
  });

  it('prefers display name then username then email', () => {
    expect(sub2apiDisplayName({ id: 1, display_name: 'A', username: 'u', email: 'e' })).toBe('A');
    expect(sub2apiDisplayName({ id: 1, username: 'u' })).toBe('u');
    expect(sub2apiDisplayName({ id: 1, email: 'e@x' })).toBe('e@x');
  });
});
