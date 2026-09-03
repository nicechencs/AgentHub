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

  it('hangs Claude and Kimi subagent paths under the parent id', () => {
    const claudeParent = session({
      id: 'claude-parent',
      relativePath: 'projects/-C-Users-demo-app/07bbb5e0-7b3e-4665-a743-4889b5efca3f.jsonl',
      sessionId: '07bbb5e0-7b3e-4665-a743-4889b5efca3f',
    });
    const claudeChild = session({
      id: 'claude-child',
      relativePath:
        'projects/-C-Users-demo-app/07bbb5e0-7b3e-4665-a743-4889b5efca3f/subagents/agent-a275.jsonl',
      sessionId: 'agent-a275',
    });
    const kimiParent = session({
      id: 'kimi-parent',
      relativePath:
        'sessions/wd/session_cc77e803-2743-4383-900d-4e2f4e054951/agents/main/wire.jsonl',
      sessionId: 'cc77e803-2743-4383-900d-4e2f4e054951',
    });
    const kimiChild = session({
      id: 'kimi-child',
      relativePath:
        'sessions/wd/session_cc77e803-2743-4383-900d-4e2f4e054951/agents/agent-0/wire.jsonl',
      sessionId: 'cc77e803-2743-4383-900d-4e2f4e054951/agent-0',
    });
    const kimiMainNotChild = session({
      id: 'kimi-main-only',
      relativePath:
        'sessions/wd/session_aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa/agents/main/wire.jsonl',
      sessionId: 'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa',
    });
    expect(cursorSubagentParentId(claudeChild)).toBe('07bbb5e0-7b3e-4665-a743-4889b5efca3f');
    expect(cursorSubagentParentId(kimiChild)).toBe('cc77e803-2743-4383-900d-4e2f4e054951');
    expect(cursorSubagentParentId(kimiMainNotChild)).toBeNull();
    const nested = nestSessions([claudeParent, claudeChild, kimiParent, kimiChild, kimiMainNotChild]);
    expect(nested.map((n) => n.session.id)).toEqual([
      'claude-parent',
      'kimi-parent',
      'kimi-main-only',
    ]);
    expect(nested[0]?.children.map((c) => c.id)).toEqual(['claude-child']);
    expect(nested[1]?.children.map((c) => c.id)).toEqual(['kimi-child']);
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
