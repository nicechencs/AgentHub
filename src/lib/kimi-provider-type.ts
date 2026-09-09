/**
 * Official Moonshot / Kimi platform roots (not `/coding` or custom relays).
 * `api.moonshot.cn`, `api.moonshot.ai`, and `api.kimi.com` match.
 */
export function isOfficialKimiPlatformUrl(url?: string | null): boolean {
  const lower = (url ?? '').trim().toLowerCase();
  if (!lower || lower.includes('/coding')) return false;
  return lower.includes('api.moonshot.') || lower.includes('api.kimi.com');
}

/**
 * Kimi Code `providers.<slug>.type` from an API root.
 *
 * anthropic → Messages; openai_responses → Responses;
 * kimi → official Moonshot / Kimi platform; openai → Chat Completions.
 */
export function kimiProviderTypeForUrl(url?: string | null): string {
  const lower = (url ?? '').trim().toLowerCase();
  if (!lower) return 'openai';
  if (
    lower.includes('/anthropic')
    || lower.includes('/v1/messages')
    || lower.endsWith('/messages')
  ) {
    return 'anthropic';
  }
  if (lower.includes('/v1/responses') || lower.includes('/responses')) {
    return 'openai_responses';
  }
  if (isOfficialKimiPlatformUrl(lower)) return 'kimi';
  return 'openai';
}
