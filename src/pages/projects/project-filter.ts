import type { AgentProject, AgentSession } from '@/lib/types';

export function sessionMatches(s: AgentSession, q: string): boolean {
  if (!q) return true;
  const hay = [
    s.sessionId ?? '',
    s.id,
    s.title,
    s.preview ?? '',
    s.cwd ?? '',
    s.path,
    s.relativePath,
  ]
    .join('\n')
    .toLowerCase();
  return hay.includes(q);
}

export function projectMatches(p: AgentProject, q: string): boolean {
  if (!q) return true;
  const hay = [
    p.title,
    p.alias ?? '',
    p.preview ?? '',
    p.actualPath ?? '',
    p.storagePath,
    p.relativePath,
  ]
    .join('\n')
    .toLowerCase();
  return hay.includes(q);
}
