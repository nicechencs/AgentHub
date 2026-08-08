/**
 * Open http(s) URLs in the **system** browser.
 *
 * Tauri webview does not reliably honor `window.open` / `<a target="_blank">`
 * for external sites. All GUI external-link clicks must go through this helper
 * (→ settings.openExternalUrl → `open_external_url` command).
 */
import { openExternalUrl } from '@/lib/api/settings';

export function isHttpUrl(url: string): boolean {
  const u = url.trim().toLowerCase();
  return u.startsWith('https://') || u.startsWith('http://');
}

export async function openExternalLink(url: string): Promise<void> {
  const trimmed = url.trim();
  if (!trimmed) {
    throw new Error('链接为空');
  }
  if (!isHttpUrl(trimmed)) {
    throw new Error(`仅支持 http(s) 链接：${trimmed}`);
  }
  await openExternalUrl(trimmed);
}

/**
 * Use on `<a href>` / button click handlers for external sites.
 * Always preventDefault so the webview never navigates in-place.
 */
export function handleExternalLinkClick(
  url: string,
  e?: { preventDefault(): void; stopPropagation?(): void },
): void {
  e?.preventDefault();
  e?.stopPropagation?.();
  void openExternalLink(url).catch((err) => {
    // Callers that need toast should use openExternalLink + try/catch.
    // This path is a last-resort for fire-and-forget anchors.
    console.error('[open-external]', err);
  });
}
