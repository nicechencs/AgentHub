import { describe, expect, it } from 'vitest';
import type { AgentSession } from '@/lib/types';
import { cursorSubagentParentId, flattenVisibleSessions, nestSessions } from './session-nest';

function session(
  partial: Partial<AgentSession> & Pick<AgentSession, 'id' | 'relativePath'>,
): AgentSession {
  return {
    projectId: 'cursor:proj:ws',
    agentId: 'cursor',
    title: partial.title ?? partial.id,
    path: `C:\\Users\\demo\\.cursor\\${partial.relativePath.replace(/\//g, '\\')}`,
    sizeBytes: 100,
    updatedAt: '2026-08-18T12:00:00.000Z',
    ...partial,
  };
}

const parentRel =
  'projects/ws/agent-transcripts/0e435bc1-cf05-4a9a-b036-8902f810bd86/0e435bc1-cf05-4a9a-b036-8902f810bd86.jsonl';
const childRel =
  'projects/ws/agent-transcripts/0e435bc1-cf05-4a9a-b036-8902f810bd86/subagents/deadbeef-0000-0000-0000-000000000001.jsonl';
const otherRel =
  'projects/ws/agent-transcripts/12411b35-6081-425a-a07a-b776a24da27a/12411b35-6081-425a-a07a-b776a24da27a.jsonl';

describe('session-nest', () => {
  it('detects Cursor subagent parent id from relativePath', () => {
    expect(cursorSubagentParentId(session({ id: 'c', relativePath: childRel }))).toBe(
      '0e435bc1-cf05-4a9a-b036-8902f810bd86',
    );
    expect(cursorSubagentParentId(session({ id: 'p', relativePath: parentRel }))).toBeNull();
  });

  it('hangs subagent rows under the parent transcript', () => {
    const parent = session({
      id: 'parent',
      relativePath: parentRel,
      sessionId: '0e435bc1-cf05-4a9a-b036-8902f810bd86',
      title: '主会话',
    });
    const child = session({
      id: 'child',
      relativePath: childRel,
      sessionId: 'deadbeef-0000-0000-0000-000000000001',
      title: '子会话',
    });
    const other = session({
      id: 'other',
      relativePath: otherRel,
      sessionId: '12411b35-6081-425a-a07a-b776a24da27a',
      title: '另一条',
    });
    const nested = nestSessions([parent, child, other]);
    expect(nested.map((n) => n.session.id)).toEqual(['parent', 'other']);
    expect(nested[0]?.children.map((c) => c.id)).toEqual(['child']);
    expect(nested[1]?.children).toEqual([]);
  });

  it('hides nested children until the parent is open', () => {
    const parent = session({
      id: 'parent',
      relativePath: parentRel,
      sessionId: '0e435bc1-cf05-4a9a-b036-8902f810bd86',
    });
    const child = session({
      id: 'child',
      relativePath: childRel,
      sessionId: 'deadbeef-0000-0000-0000-000000000001',
    });
    expect(flattenVisibleSessions([parent, child], new Set()).map((s) => s.id)).toEqual(['parent']);
    expect(
      flattenVisibleSessions([parent, child], new Set(['parent'])).map((s) => s.id),
    ).toEqual(['parent', 'child']);
  });

  it('keeps a subagent as a root when the parent is missing', () => {
    const child = session({
      id: 'child',
      relativePath: childRel,
      sessionId: 'deadbeef-0000-0000-0000-000000000001',
    });
    const nested = nestSessions([child]);
    expect(nested).toHaveLength(1);
    expect(nested[0]?.session.id).toBe('child');
    expect(nested[0]?.children).toEqual([]);
  });
});
