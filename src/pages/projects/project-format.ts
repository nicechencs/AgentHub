import type { AgentProject, AgentSession } from '@/lib/types';

export function displayTitle(p: AgentProject): string {
  const a = p.alias?.trim();
  return a || p.title;
}

export function fmtBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / (1024 * 1024)).toFixed(1)} MB`;
}

export function relativeTime(iso: string): string {
  const t = Date.parse(iso.includes('T') ? iso : iso.replace(' ', 'T') + 'Z');
  if (Number.isNaN(t)) return '';
  const diff = Date.now() - t;
  const m = Math.floor(diff / 60000);
  if (m < 1) return '刚刚';
  if (m < 60) return `${m} 分钟前`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h} 小时前`;
  const d = Math.floor(h / 24);
  if (d < 30) return `${d} 天前`;
  return new Date(t).toLocaleDateString();
}

export function shortPath(p: string, max = 48): string {
  if (p.length <= max) return p;
  return `…${p.slice(-(max - 1))}`;
}

export function nativeSessionId(s: AgentSession): string | null {
  const sid = s.sessionId?.trim();
  return sid ? sid : null;
}

export function shortSessionId(id: string, max = 36): string {
  if (id.length <= max) return id;
  return `${id.slice(0, max - 1)}…`;
}
