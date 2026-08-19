import { describe, expect, it } from 'vitest';
import { envOneClickInstallVariant } from './env-remediation-cta';

describe('envOneClickInstallVariant', () => {
  it('returns default when the page has no primary CTA', () => {
    expect(envOneClickInstallVariant(false)).toBe('default');
  });

  it('returns secondary when the page already has a primary CTA', () => {
    expect(envOneClickInstallVariant(true)).toBe('secondary');
  });
});
