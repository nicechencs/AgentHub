import { describe, expect, it } from 'vitest';
import {
  maskEmail,
  normalizeHttpBaseUrl,
  normalizeSiteUrl,
  sub2apiApiRoot,
  sub2apiGatewayBaseUrl,
  sub2apiLoginUrl,
  tryNormalizeSiteUrl,
} from './url';

describe('sub2api url helpers', () => {
  it('normalizes bare hosts to https and strips trailing slash', () => {
    expect(normalizeSiteUrl('v2.pincc.ai')).toBe('https://v2.pincc.ai');
    expect(normalizeSiteUrl('https://v2.pincc.ai/')).toBe('https://v2.pincc.ai');
  });

  it('strips /login and other paths to scheme+host[+port]', () => {
    expect(normalizeSiteUrl('https://v2.pincc.ai/login')).toBe('https://v2.pincc.ai');
    expect(normalizeSiteUrl('https://v2.pincc.ai/api/v1')).toBe('https://v2.pincc.ai');
    expect(normalizeSiteUrl('https://v2.pincc.ai:8443/foo/bar?x=1#h')).toBe(
      'https://v2.pincc.ai:8443',
    );
    const stripped = tryNormalizeSiteUrl('https://v2.pincc.ai/login');
    expect(stripped).toEqual({ ok: true, url: 'https://v2.pincc.ai', stripped: true });
    const clean = tryNormalizeSiteUrl('https://v2.pincc.ai');
    expect(clean).toEqual({ ok: true, url: 'https://v2.pincc.ai', stripped: false });
  });

  it('reports empty and invalid pastes without falling back inside tryNormalize', () => {
    expect(tryNormalizeSiteUrl('')).toEqual({ ok: false, reason: 'empty' });
    expect(tryNormalizeSiteUrl('   ')).toEqual({ ok: false, reason: 'empty' });
    expect(tryNormalizeSiteUrl('not a url :::')).toEqual({ ok: false, reason: 'invalid' });
    expect(tryNormalizeSiteUrl('ftp://v2.pincc.ai')).toEqual({ ok: false, reason: 'invalid' });
  });

  it('builds api and login paths from the site root', () => {
    expect(sub2apiApiRoot('https://v2.pincc.ai')).toBe('https://v2.pincc.ai/api/v1');
    expect(sub2apiApiRoot('https://v2.pincc.ai/login')).toBe('https://v2.pincc.ai/api/v1');
    expect(sub2apiLoginUrl('https://v2.pincc.ai')).toBe('https://v2.pincc.ai/login');
  });

  it('keeps path on gateway bases from public api_base_url', () => {
    expect(normalizeHttpBaseUrl('https://gw.example/v1/')).toBe('https://gw.example/v1');
    expect(
      sub2apiGatewayBaseUrl('https://v2.pincc.ai', { api_base_url: 'https://gw.example/v1/' }),
    ).toBe('https://gw.example/v1');
    expect(sub2apiGatewayBaseUrl('https://v2.pincc.ai', { api_base_url: '' })).toBe(
      'https://v2.pincc.ai',
    );
  });

  it('masks emails for account picker rows', () => {
    expect(maskEmail('alice@example.com')).toBe('al***@example.com');
    expect(maskEmail('ab@x.co')).toBe('a***@x.co');
    expect(maskEmail('a@x.co')).toBe('a***@x.co');
  });
});
