import { describe, expect, it } from 'vitest';
import {
  formatGroupRate,
  formatKeyExpires,
  formatKeyModels,
  formatKeyModelsFromKey,
  formatKeyQuota,
  formatKeyTableTimestamp,
  formatKeyTimestamp,
  formatUsdAmount,
  maskSub2ApiTableKey,
  normalizeTotpCode,
  pickGroupLabel,
  pickGroupRate,
  pickKeyConcurrency,
  pickKeyUsageUsd,
  sortSub2ApiKeys,
  sub2apiDisplayName,
  sub2apiKeyStatusBadgeVariant,
  sub2apiKeyStatusKind,
  sub2apiKeyStatusLabel,
  sub2apiPagePhase,
} from './sub2api-page-model';

describe('sub2api page model', () => {
  it('maps page states including quiet restore', () => {
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
    // Restoring wins so the login form does not flash
    expect(
      sub2apiPagePhase(
        { siteUrl: 'https://x', gatewayBaseUrl: 'https://x', accessToken: 't' },
        false,
        true,
      ),
    ).toBe('restoring');
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
    expect(sub2apiKeyStatusKind('expired')).toBe('expired');
    expect(sub2apiKeyStatusKind('quota_exhausted')).toBe('quota_exhausted');
    expect(sub2apiKeyStatusKind('weird')).toBe('other');
    expect(sub2apiKeyStatusBadgeVariant('active')).toBe('success');
    expect(sub2apiKeyStatusBadgeVariant('quota_exhausted')).toBe('danger');
    expect(
      sub2apiKeyStatusLabel('disabled', {
        active: 'Active',
        disabled: 'Disabled',
        other: 'Other',
      }),
    ).toBe('Disabled');
    expect(
      sub2apiKeyStatusLabel('quota_exhausted', {
        active: 'Active',
        disabled: 'Disabled',
        other: 'Other',
        quotaExhausted: 'Quota used up',
      }),
    ).toBe('Quota used up');
  });

  it('formats key timestamps as local YYYY-MM-DD HH:mm', () => {
    expect(formatKeyTimestamp(null)).toBeNull();
    expect(formatKeyTimestamp('')).toBeNull();
    expect(formatKeyTimestamp('not-a-date')).toBeNull();
    const formatted = formatKeyTimestamp('2026-09-04T04:00:00.000Z');
    expect(formatted).toMatch(/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}$/);
    const table = formatKeyTableTimestamp('2026-09-04T04:00:00.000Z');
    expect(table).toMatch(/^\d{4}\/\d{2}\/\d{2} \d{2}:\d{2}:\d{2}$/);
    expect(formatKeyExpires(null, 'Never')).toBe('Never');
    expect(formatKeyExpires(-1, 'Never')).toBe('Never');
    expect(formatKeyExpires('never', 'Never')).toBe('Never');
  });

  it('formats quota / usage labels', () => {
    expect(formatKeyQuota({ id: 1, key: 'k', name: 'n', status: 'active', unlimited_quota: true }, { unlimited: 'Unlimited' })).toBe(
      'Unlimited',
    );
    expect(
      formatKeyQuota({ id: 1, key: 'k', name: 'n', status: 'active', used_quota: 1000, quota: 5000 }, { unlimited: 'Unlimited' }),
    ).toBe('1,000 / 5,000');
    expect(
      formatKeyQuota({ id: 1, key: 'k', name: 'n', status: 'active', quota_used: 1.5, quota: 10 }, { unlimited: 'Unlimited' }),
    ).toBe('1.5 / 10');
    expect(formatKeyQuota({ id: 1, key: 'k', name: 'n', status: 'active', quota: 0 }, { unlimited: 'Unlimited' })).toBe('Unlimited');
    expect(formatKeyQuota({ id: 1, key: 'k', name: 'n', status: 'active', remain_quota: 42 }, { unlimited: 'Unlimited' })).toBe('42');
    expect(formatKeyQuota({ id: 1, key: 'k', name: 'n', status: 'active' }, { unlimited: 'Unlimited' })).toBeNull();
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
    expect(pickGroupLabel({ id: 1, key: 'k', name: 'n', status: 'active', group_name: 'default', group_id: 1 })).toBe('default');
    expect(pickGroupLabel({ id: 1, key: 'k', name: 'n', status: 'active', group: 'vip', group_id: 2 })).toBe('vip');
    expect(pickGroupLabel({ id: 1, key: 'k', name: 'n', status: 'active', group: { id: 3, name: 'Claude Pro' }, group_id: 3 })).toBe('Claude Pro');
    expect(pickGroupLabel({ id: 1, key: 'k', name: 'n', status: 'active', group_id: 9 })).toBe('9');
    expect(pickGroupLabel({ id: 1, key: 'k', name: 'n', status: 'active' })).toBeNull();
  });

  it('reads models from nested group config', () => {
    expect(
      formatKeyModelsFromKey({
        id: 1,
        key: 'k',
        name: 'n',
        status: 'active',
        group: { models_list_config: { enabled: true, models: ['a', 'b'] } },
      }),
    ).toBe('a, b');
  });

  it('sorts keys by created time newest first', () => {
    const sorted = sortSub2ApiKeys([
      { id: 1, key: 'a', name: 'old', status: 'active', created_at: '2026-01-01T00:00:00.000Z' },
      { id: 2, key: 'b', name: 'new', status: 'active', created_at: '2026-09-01T00:00:00.000Z' },
    ]);
    expect(sorted.map((k) => k.id)).toEqual([2, 1]);
  });

  it('reads concurrency, USD usage, group rate, and table key mask', () => {
    expect(pickKeyConcurrency({ id: 1, key: 'k', name: 'n', status: 'active' })).toBe(0);
    expect(
      pickKeyConcurrency({ id: 1, key: 'k', name: 'n', status: 'active', current_concurrency: 3 }),
    ).toBe(3);
    expect(
      pickKeyUsageUsd({
        id: 1,
        key: 'k',
        name: 'n',
        status: 'active',
        today_usage: 1.2,
        last_30_days_usage: 4,
      }),
    ).toEqual({ today: 1.2, last30Days: 4 });
    expect(formatUsdAmount(0)).toBe('$0.0000');
    expect(formatGroupRate(2.1)).toBe('2.1x');
    expect(formatGroupRate(0.4)).toBe('0.4x');
    expect(
      pickGroupRate({
        id: 1,
        key: 'k',
        name: 'n',
        status: 'active',
        group: { name: 'Claude Max', rate: 2.1 },
      }),
    ).toBe('2.1x');
    expect(maskSub2ApiTableKey('sk-c33abcdefgh62e2')).toBe('sk-c33...62e2');
  });

  it('skips nested models when models_list_config.enabled is false', () => {
    expect(
      formatKeyModelsFromKey({
        id: 1,
        key: 'k',
        name: 'n',
        status: 'active',
        group: { models_list_config: { enabled: false, models: ['a', 'b'] } },
      }),
    ).toBeNull();
  });
});
