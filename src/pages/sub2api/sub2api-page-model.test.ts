import { describe, expect, it } from 'vitest';
import {
  normalizeTotpCode,
  sub2apiDisplayName,
  sub2apiPagePhase,
} from './sub2api-page-model';

describe('sub2api page model', () => {
  it('maps three page states: logged-out / awaiting-2fa / logged-in', () => {
    expect(sub2apiPagePhase(null, true)).toBe('awaiting-2fa');
    expect(
      sub2apiPagePhase(
        { siteUrl: 'https://x', gatewayBaseUrl: 'https://x', accessToken: 't' },
        false,
      ),
    ).toBe('logged-in');
    expect(sub2apiPagePhase(null, false)).toBe('logged-out');
    // Logged-in wins over awaiting-2fa
    expect(
      sub2apiPagePhase(
        { siteUrl: 'https://x', gatewayBaseUrl: 'https://x', accessToken: 't' },
        true,
      ),
    ).toBe('logged-in');
  });

  it('prefers display name then username then email', () => {
    expect(sub2apiDisplayName({ id: 1, display_name: 'A', username: 'u', email: 'e' })).toBe('A');
    expect(sub2apiDisplayName({ id: 1, username: 'u' })).toBe('u');
    expect(sub2apiDisplayName({ id: 1, email: 'e@x' })).toBe('e@x');
  });

  it('normalizes TOTP to 6 digits', () => {
    expect(normalizeTotpCode('12 34-56')).toBe('123456');
    expect(normalizeTotpCode('abcdef')).toBe('');
    expect(normalizeTotpCode('123456789')).toBe('123456');
  });
});
