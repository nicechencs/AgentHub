import { describe, expect, it } from 'vitest';
import {
  isPiRefreshProvider,
  OFFICIAL_LOGIN_SUPERSEDED,
  OAUTH_PKCE_LISTEN_TIMEOUT_SECS,
  OAUTH_WAIT_TIMEOUT_SECS,
  PI_REFRESH_PROVIDER_ALIASES,
  PI_REFRESH_PROVIDERS,
} from './oauth-constants';

describe('oauth wait windows', () => {
  it('keeps the poll chunk shorter than the PKCE listener', () => {
    expect(OAUTH_WAIT_TIMEOUT_SECS).toBe(120);
    expect(OAUTH_PKCE_LISTEN_TIMEOUT_SECS).toBe(900);
    expect(OAUTH_PKCE_LISTEN_TIMEOUT_SECS).toBeGreaterThan(OAUTH_WAIT_TIMEOUT_SECS);
    expect(OFFICIAL_LOGIN_SUPERSEDED).toBe('oauth.superseded');
  });
});

describe('oauth-constants Pi refresh mirror', () => {
  it('locks the frozen alias set shared with Rust pi_refreshable_provider_aliases', () => {
    // Keep sorted — matches Rust Vec sort + dedup order.
    expect([...PI_REFRESH_PROVIDER_ALIASES]).toEqual([
      'anthropic',
      'claude',
      'codex',
      'grok',
      'openai',
      'openai-codex',
      'xai',
    ]);
    expect(PI_REFRESH_PROVIDERS.size).toBe(PI_REFRESH_PROVIDER_ALIASES.length);
  });

  it('matches aliases case-insensitively and rejects unknown providers', () => {
    expect(isPiRefreshProvider('Claude')).toBe(true);
    expect(isPiRefreshProvider(' OPENAI ')).toBe(true);
    expect(isPiRefreshProvider('openrouter')).toBe(false);
    expect(isPiRefreshProvider(null)).toBe(false);
    expect(isPiRefreshProvider('')).toBe(false);
  });
});
