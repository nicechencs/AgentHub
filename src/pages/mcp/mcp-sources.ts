import type { McpSourceFile } from '@/lib/backend/contracts/mcp-types';

/** Existing or unreadable configs. Probe paths that were never created stay hidden. */
export function isVisibleMcpSource(source: McpSourceFile): boolean {
  return source.exists || Boolean(source.error?.trim());
}

export function visibleMcpSources(
  sources: readonly McpSourceFile[] | undefined,
): McpSourceFile[] {
  return (sources ?? []).filter(isVisibleMcpSource);
}
