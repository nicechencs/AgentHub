/**
 * Shared OAuth client constants.
 * Keep wire-compatible with backend defaults (oauth_wait timeout_secs).
 */

/** One waitOAuth / oauth_wait poll chunk (seconds). Does not end the session. */
export const OAUTH_WAIT_TIMEOUT_SECS = 120;

/** PKCE listener lifetime. Wait-page countdown must match this, not the poll chunk. */
export const OAUTH_PKCE_LISTEN_TIMEOUT_SECS = 900;

/** Another official login took the shared loopback port. */
export const OFFICIAL_LOGIN_SUPERSEDED = 'oauth.superseded';

/**
 * Pi OAuth provider keys/aliases for which AgentHub implements credential refresh.
 *
 * Frozen mirror of
 * `crates/agenthub-core/src/oauth/catalog.rs` → `pi_refreshable_provider_aliases()`.
 * When the Rust table changes, update this set and both unit tests together.
 */
export const PI_REFRESH_PROVIDER_ALIASES = [
  'anthropic',
  'claude',
  'codex',
  'grok',
  'openai',
  'openai-codex',
  'xai',
] as const;

export type PiRefreshProviderAlias = (typeof PI_REFRESH_PROVIDER_ALIASES)[number];

export const PI_REFRESH_PROVIDERS = new Set<string>(PI_REFRESH_PROVIDER_ALIASES);

/** Case-insensitive lookup against {@link PI_REFRESH_PROVIDERS}. */
export function isPiRefreshProvider(provider: string | null | undefined): boolean {
  if (!provider) return false;
  return PI_REFRESH_PROVIDERS.has(provider.trim().toLowerCase());
}
