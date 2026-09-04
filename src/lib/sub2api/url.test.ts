import { describe, expect, it } from 'vitest';
import {
  normalizeSiteUrl,
  sub2apiApiRoot,
  sub2apiGatewayBaseUrl,
  sub2apiLoginUrl,
} from './url';

describe('sub2api url helpers', () => {
  it('normalizes bare hosts to https and strips trailing slash', () => {
    expect(normalizeSiteUrl('v2.pincc.ai')).toBe('https://v2.pincc.ai');
    expect(normalizeSiteUrl('https://v2.pincc.ai/')).toBe('https://v2.pincc.ai');
  });

  it('builds api and login paths from the site root', () => {
    expect(sub2apiApiRoot('https://v2.pincc.ai')).toBe('https://v2.pincc.ai/api/v1');
    expect(sub2apiLoginUrl('https://v2.pincc.ai')).toBe('https://v2.pincc.ai/login');
  });

  it('prefers public api_base_url for the Connections gateway base', () => {
    expect(
      sub2apiGatewayBaseUrl('https://v2.pincc.ai', { api_base_url: 'https://gw.example/v1/' }),
    ).toBe('https://gw.example/v1');
    expect(sub2apiGatewayBaseUrl('https://v2.pincc.ai', { api_base_url: '' })).toBe(
      'https://v2.pincc.ai',
    );
  });
});
