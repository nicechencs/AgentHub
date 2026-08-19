import type { TranslateFn } from '@/lib/i18n';
import { restoreProjectWorkspacePath } from '@/lib/path-open';
import type { AgentProject, AgentSession } from '@/lib/types';

export function displayTitle(p: Pick<AgentProject, 'title' | 'alias'>): string {
  const a = p.alias?.trim();
  return a || p.title;
}

/** Restored address for display. Click-to-open still requires a verified actualPath. */
export function projectDisplayPath(
  p: Pick<AgentProject, 'agentId' | 'actualPath' | 'relativePath' | 'storagePath'>,
): string {
  const restored = restoreProjectWorkspacePath(p);
  if (restored) return restored;
  const actual = p.actualPath?.trim();
  if (actual) return actual;
  const rel = p.relativePath?.trim();
  if (rel) return rel;
  return p.storagePath;
}

/**
 * Preview text that still adds something beyond the visible title.
 * If preview continues the title, only the remainder is returned.
 */
export function extraPreview(title: string, preview?: string | null): string | null {
  const p = preview?.trim();
  if (!p) return null;
  const t = title.trim();
  if (!t) return p;
  const fold = (s: string) => s.replace(/[….]+$/u, '').trim();
  const tf = fold(t);
  const pf = fold(p);
  if (!tf) return p;
  const tfl = tf.toLowerCase();
  const pfl = pf.toLowerCase();
  if (pfl === tfl) return null;
  if (tfl.startsWith(pfl)) return null;
  if (pfl.startsWith(tfl)) {
    const rest = pf.slice(tf.length).replace(/^[,，.。;；:：\s—-]+/u, '').trim();
    return rest || null;
  }
  return p;
}

/** Truncated titles still need the full string; leftover preview is appended. */
export function titleHoverLabel(title: string, preview?: string | null): string {
  const extra = extraPreview(title, preview);
  return extra ? `${title}\n${extra}` : title;
}

export function fmtBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / (1024 * 1024)).toFixed(1)} MB`;
}

export function relativeTime(iso: string, t: TranslateFn): string {
  const parsed = Date.parse(iso.includes('T') ? iso : iso.replace(' ', 'T') + 'Z');
  if (Number.isNaN(parsed)) return '';
  const diff = Date.now() - parsed;
  const m = Math.floor(diff / 60000);
  if (m < 1) return t('projects.time.justNow');
  if (m < 60) return t('projects.time.minutes', { n: m });
  const h = Math.floor(m / 60);
  if (h < 24) return t('projects.time.hours', { n: h });
  const d = Math.floor(h / 24);
  if (d < 30) return t('projects.time.days', { n: d });
  return new Date(parsed).toLocaleDateString();
}

export function shortPath(p: string, max = 48): string {
  if (p.length <= max) return p;
  return `…${p.slice(-(max - 1))}`;
}

export function nativeSessionId(s: Pick<AgentSession, 'sessionId'>): string | null {
  const sid = s.sessionId?.trim();
  return sid ? sid : null;
}

export function shortSessionId(id: string, max = 36): string {
  if (id.length <= max) return id;
  return `${id.slice(0, max - 1)}…`;
}
