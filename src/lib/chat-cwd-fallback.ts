/** Suggested live folder when a conversation's stored cwd is gone. */
const fallbackCwdByConversation = new Map<string, string>();

export function rememberFallbackCwd(conversationId: string, cwd: string): void {
  const trimmed = cwd.trim();
  if (!trimmed) return;
  fallbackCwdByConversation.set(conversationId, trimmed);
}

export function peekFallbackCwd(conversationId: string): string | null {
  return fallbackCwdByConversation.get(conversationId) ?? null;
}

export function forgetFallbackCwd(conversationId: string): void {
  fallbackCwdByConversation.delete(conversationId);
}