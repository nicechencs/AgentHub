import { describe, expect, it } from 'vitest';
import {
  formatKeyModels,
  formatKeyQuota,
  formatKeyTimestamp,
  normalizeTotpCode,
  pickGroupLabel,
  sub2apiDisplayName,
  sub2apiKeyStatusKind,
  sub2apiKeyStatusLabel,
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

  it('classifies key status kinds and labels', () => {
    expect(sub2apiKeyStatusKind('active')).toBe('active');
    expect(sub2apiKeyStatusKind('enabled')).toBe('active');
    expect(sub2apiKeyStatusKind(1)).toBe('active');
    expect(sub2apiKeyStatusKind('disabled')).toBe('disabled');
    expect(sub2apiKeyStatusKind('2')).toBe('disabled');
    expect(sub2apiKeyStatusKind('weird')).toBe('other');
    expect(
      sub2apiKeyStatusLabel('disabled', {
        active: 'Active',
        disabled: 'Disabled',
        other: 'Other',
      }),
    ).toBe('Disabled');
  });

  it('formats key timestamps as local YYYY-MM-DD HH:mm', () => {
    expect(formatKeyTimestamp(null)).toBeNull();
    expect(formatKeyTimestamp('')).toBeNull();
    expect(formatKeyTimestamp('not-a-date')).toBeNull();
    const formatted = formatKeyTimestamp('2026-09-04T04:00:00.000Z');
    expect(formatted).toMatch(/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}$/);
  });

  it('formats quota / usage labels', () => {
    expect(formatKeyQuota({ unlimited_quota: true }, { unlimited: 'Unlimited' })).toBe(
      'Unlimited',
    );
    expect(
      formatKeyQuota({ used_quota: 1000, quota: 5000 }, { unlimited: 'Unlimited' }),
    ).toBe('1,000 / 5,000');
    expect(formatKeyQuota({ remain_quota: 42 }, { unlimited: 'Unlimited' })).toBe('42');
    expect(formatKeyQuota({}, { unlimited: 'Unlimited' })).toBeNull();
  });

  it('formats models list and truncates', () => {
    expect(formatKeyModels(null)).toBeNull();
    expect(formatKeyModels('claude-sonnet-4')).toBe('claude-sonnet-4');
    expect(formatKeyModels('a, b, c')).toBe('a, b, c');
    expect(formatKeyModels(['m1', 'm2', 'm3', 'm4', 'm5', 'm6', 'm7'], 3)).toBe(
      'm1, m2, m3 (+4)',
    );
  });

  it('picks group label from name, group, or id', () => {
    expect(pickGroupLabel({ group_name: 'default', group_id: 1 })).toBe('default');
    expect(pickGroupLabel({ group: 'vip', group_id: 2 })).toBe('vip');
    expect(pickGroupLabel({ group_id: 9 })).toBe('9');
    expect(pickGroupLabel({})).toBeNull();
  });
});
