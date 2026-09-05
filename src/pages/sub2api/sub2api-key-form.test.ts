import { describe, expect, it } from 'vitest';
import type { Sub2ApiKey } from '@/lib/sub2api';
import {
  addDaysToDateTimeLocal,
  buildEditPatch,
  formFromKey,
  formatDateTimeLocal,
  formatIpList,
  parseIpList,
  parseUsdField,
  pickQuotaLimit,
  pickQuotaUsed,
  pickRateWindow,
  rateUsagePercent,
  rateUsageTone,
  shouldSubmitEditStatus,
} from './sub2api-key-form';

const key = (over: Partial<Sub2ApiKey> = {}): Sub2ApiKey => ({
  id: 9,
  key: 'sk-test',
  name: 'demo',
  status: 'active',
  group_id: 3,
  ...over,
});

describe('sub2api key form', () => {
  it('parses IP lists and USD fields', () => {
    expect(parseIpList(' 1.1.1.1 \n\n10.0.0.0/8 ')).toEqual(['1.1.1.1', '10.0.0.0/8']);
    expect(formatIpList(['1.1.1.1', '10.0.0.0/8'])).toBe('1.1.1.1\n10.0.0.0/8');
    expect(parseUsdField('')).toBe(0);
    expect(parseUsdField('0')).toBe(0);
    expect(parseUsdField('12.5')).toBe(12.5);
  });

  it('loads the edit form from a key like Sub2API KeysView', () => {
    const form = formFromKey(
      key({
        status: 'quota_exhausted',
        ip_whitelist: ['1.1.1.1'],
        quota: 10,
        rate_limit_1d: 5,
        expires_at: '2026-12-01T08:00:00.000Z',
      }),
    );
    expect(form.status).toBe('inactive');
    expect(form.enableIpRestriction).toBe(true);
    expect(form.ipWhitelist).toBe('1.1.1.1');
    expect(form.quota).toBe('10');
    expect(form.enableRateLimit).toBe(true);
    expect(form.rateLimit1d).toBe('5');
    expect(form.enableExpiration).toBe(true);
    expect(form.expirationPreset).toBe('custom');
    expect(form.expirationDate).toMatch(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}$/);
  });

  it('builds the same PUT payload shape as Sub2API', () => {
    const form = formFromKey(
      key({
        ip_whitelist: ['1.1.1.1'],
        quota: 8,
        rate_limit_5h: 1,
        expires_at: '2026-12-01T08:00:00.000Z',
      }),
    );
    const patch = buildEditPatch(form, key());
    expect(patch.name).toBe('demo');
    expect(patch.group_id).toBe(3);
    expect(patch.status).toBe('active');
    expect(patch.ip_whitelist).toEqual(['1.1.1.1']);
    expect(patch.ip_blacklist).toEqual([]);
    expect(patch.quota).toBe(8);
    expect(patch.rate_limit_5h).toBe(1);
    expect(patch.rate_limit_1d).toBe(0);
    expect(patch.expires_at).toMatch(/Z$/);
  });

  it('clears IP, quota, rate limit and expiration when toggles are off', () => {
    const form = formFromKey(
      key({
        ip_whitelist: ['1.1.1.1'],
        quota: 8,
        rate_limit_5h: 1,
        expires_at: '2026-12-01T08:00:00.000Z',
      }),
    );
    form.enableIpRestriction = false;
    form.enableRateLimit = false;
    form.enableExpiration = false;
    form.quota = '';
    const patch = buildEditPatch(form, key());
    expect(patch.ip_whitelist).toEqual([]);
    expect(patch.ip_blacklist).toEqual([]);
    expect(patch.quota).toBe(0);
    expect(patch.rate_limit_5h).toBe(0);
    expect(patch.rate_limit_1d).toBe(0);
    expect(patch.rate_limit_7d).toBe(0);
    expect(patch.expires_at).toBe('');
  });

  it('only submits status for exhausted/expired keys when turning them active', () => {
    expect(shouldSubmitEditStatus('quota_exhausted', 'inactive')).toBe(false);
    expect(shouldSubmitEditStatus('expired', 'active')).toBe(true);
    expect(shouldSubmitEditStatus('active', 'inactive')).toBe(true);
    const patch = buildEditPatch(formFromKey(key({ status: 'quota_exhausted' })), key({ status: 'quota_exhausted' }));
    expect(patch.status).toBeUndefined();
  });

  it('formats datetime-local and extends by days from now', () => {
    expect(formatDateTimeLocal('2026-12-01T08:00:00.000Z')).toMatch(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}$/);
    const from = new Date('2026-01-01T00:00:00');
    expect(addDaysToDateTimeLocal(7, from)).toBe('2026-01-08T00:00');
  });

  it('reads quota and rate-limit usage for the edit panel', () => {
    const row = key({ quota: 10, quota_used: 1.5, rate_limit_5h: 2, usage_5h: 1.8 });
    expect(pickQuotaLimit(row)).toBe(10);
    expect(pickQuotaUsed(row)).toBe(1.5);
    expect(pickRateWindow(row, '5h')).toEqual({ limit: 2, used: 1.8 });
    expect(rateUsagePercent(1.8, 2)).toBe(90);
    expect(rateUsageTone(1.8, 2)).toBe('warn');
    expect(rateUsageTone(2, 2)).toBe('over');
  });
});
