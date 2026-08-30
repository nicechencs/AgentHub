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
  if (
    !lower.includes('/coding')
    && (lower.includes('api.moonshot.') || lower.includes('api.kimi.com'))
  ) {
    return 'kimi';
  }
  return 'openai';
}
