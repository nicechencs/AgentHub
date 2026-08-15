import { describe, expect, it } from 'vitest';
import {
  isPiRefreshProvider,
  PI_REFRESH_PROVIDER_ALIASES,
  PI_REFRESH_PROVIDERS,
} from './oauth-constants';

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
