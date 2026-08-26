/**
 * Shared OAuth client constants.
 * Keep wire-compatible with backend defaults (oauth_wait timeout_secs).
 */

/** Default PKCE wait window (seconds) for waitOAuth / completeOAuth. */
export const OAUTH_WAIT_TIMEOUT_SECS = 120;

/** Agents with an in-app OAuth login option (mirrors core `list_oauth_options`). */
export const OAUTH_LOGIN_AGENT_IDS = ['claude', 'codex', 'grok', 'pi'] as const;

export function agentSupportsOAuthLogin(agentId: string): boolean {
  return (OAUTH_LOGIN_AGENT_IDS as readonly string[]).includes(agentId);
}

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
