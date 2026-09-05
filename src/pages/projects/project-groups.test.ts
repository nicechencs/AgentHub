import { describe, expect, it } from 'vitest';
import type { AgentProject, AgentSession } from '@/lib/types';
import {
  groupCanExpand,
  groupProjectsByPath,
  normalizeProjectMergePath,
  parseProjectSortKey,
  projectMergeKey,
  sortProjectGroups,
  sortSessions,
} from './project-groups';

function project(
  partial: Partial<AgentProject> & Pick<AgentProject, 'id' | 'agentId' | 'title'>,
): AgentProject {
  return {
    storagePath: `C:\\Users\\demo\\.${partial.agentId}\\projects\\${partial.id}`,
    relativePath: `projects/${partial.id}`,
    sessionCount: 1,
    sizeBytes: 10,
    updatedAt: '2026-08-18T12:00:00.000Z',
    preview: null,
    ...partial,
  };
}

function session(
  partial: Partial<AgentSession> & Pick<AgentSession, 'id' | 'projectId' | 'agentId' | 'title'>,
): AgentSession {
  return {
    path: `C:\\Users\\demo\\.${partial.agentId}\\${partial.id}.jsonl`,
    relativePath: `${partial.id}.jsonl`,
    sizeBytes: 8,
    updatedAt: '2026-08-18T12:00:00.000Z',
    preview: null,
    ...partial,
  };
}

describe('parseProjectSortKey', () => {
  it('keeps known keys and falls back to time', () => {
    expect(parseProjectSortKey('time')).toBe('time');
    expect(parseProjectSortKey('agent')).toBe('agent');
    expect(parseProjectSortKey('name')).toBe('name');
    expect(parseProjectSortKey('')).toBe('time');
    expect(parseProjectSortKey('nope')).toBe('time');
    expect(parseProjectSortKey(null)).toBe('time');
  });
});

describe('groupProjectsByPath', () => {
  const claudeApp = project({
    id: 'claude:proj:app',
    agentId: 'claude',
    title: 'app',
    actualPath: 'C:\\Users\\demo\\app',
    updatedAt: '2026-08-18T12:00:00.000Z',
    sessionCount: 2,
    sizeBytes: 20,
  });
  const grokApp = project({
    id: 'grok:proj:app',
    agentId: 'grok',
    title: 'app',
    actualPath: 'c:/Users/demo/app/',
    updatedAt: '2026-08-19T08:00:00.000Z',
    sessionCount: 1,
    sizeBytes: 8,
  });
  const kimiLoose = project({
    id: 'kimi:proj:__ungrouped__',
    agentId: 'kimi',
    title: '未分类会话',
    actualPath: null,
    relativePath: 'sessions',
  });
  const grokLoose = project({
    id: 'grok:proj:__ungrouped__',
    agentId: 'grok',
    title: '未分类会话',
    actualPath: null,
    relativePath: 'sessions',
  });

  it('normalizes slash, case, and trailing separators', () => {
    expect(normalizeProjectMergePath('C:\\Users\\demo\\app\\')).toBe('c:/users/demo/app');
    expect(projectMergeKey(claudeApp)).toBe(projectMergeKey(grokApp));
  });

  it('keeps one card per project when merge is off', () => {
    const groups = groupProjectsByPath([claudeApp, grokApp], false);
    expect(groups.map((g) => g.id)).toEqual([claudeApp.id, grokApp.id]);
    expect(groups.every((g) => g.members.length === 1)).toBe(true);
  });

  it('merges the same workspace across agents and sums session details', () => {
    const [group] = groupProjectsByPath([claudeApp, grokApp], true);
    expect(group.members.map((m) => m.id).sort()).toEqual([claudeApp.id, grokApp.id].sort());
    expect(group.agentIds).toEqual(['claude', 'grok']);
    expect(group.sessionCount).toBe(3);
    expect(group.sizeBytes).toBe(28);
    expect(group.updatedAt).toBe(grokApp.updatedAt);
    expect(group.primary).toBe(grokApp);
    expect(group.id.startsWith('path:')).toBe(true);
  });

  it('does not merge ungrouped rows that only share a relative key', () => {
    const groups = groupProjectsByPath([kimiLoose, grokLoose], true);
    expect(groups).toHaveLength(2);
    expect(groups.map((g) => g.id).sort()).toEqual([kimiLoose.id, grokLoose.id].sort());
  });

  it('marks a group hidden only when every member is hidden', () => {
    const mixed = groupProjectsByPath(
      [claudeApp, { ...grokApp, hidden: true }],
      true,
    )[0];
    expect(mixed.hidden).toBe(false);
    const hidden = groupProjectsByPath(
      [
        { ...claudeApp, hidden: true },
        { ...grokApp, hidden: true },
      ],
      true,
    )[0];
    expect(hidden.hidden).toBe(true);
  });
});

describe('sortProjectGroups / sortSessions', () => {
  const older = project({
    id: 'claude:proj:docs',
    agentId: 'claude',
    title: 'docs',
    actualPath: 'C:\\Users\\demo\\docs',
    updatedAt: '2026-01-01T00:00:00.000Z',
  });
  const newer = project({
    id: 'grok:proj:app',
    agentId: 'grok',
    title: 'app',
    actualPath: 'C:\\Users\\demo\\app',
    updatedAt: '2026-08-19T00:00:00.000Z',
  });

  it('sorts groups by time, name, and agent', () => {
    const groups = groupProjectsByPath([older, newer], false);
    expect(sortProjectGroups(groups, 'time').map((g) => g.primary.title)).toEqual(['app', 'docs']);
    expect(sortProjectGroups(groups, 'name').map((g) => g.primary.title)).toEqual(['app', 'docs']);
    expect(sortProjectGroups(groups, 'agent').map((g) => g.primary.agentId)).toEqual([
      'claude',
      'grok',
    ]);
  });

  it('sorts sessions by time, name, and agent', () => {
    const a = session({
      id: 'a',
      projectId: older.id,
      agentId: 'claude',
      title: 'zeta',
      updatedAt: '2026-08-19T00:00:00.000Z',
    });
    const b = session({
      id: 'b',
      projectId: newer.id,
      agentId: 'grok',
      title: 'alpha',
      updatedAt: '2026-01-01T00:00:00.000Z',
    });
    expect(sortSessions([a, b], 'time').map((s) => s.id)).toEqual(['a', 'b']);
    expect(sortSessions([a, b], 'name').map((s) => s.id)).toEqual(['b', 'a']);
    expect(sortSessions([a, b], 'agent').map((s) => s.agentId)).toEqual(['claude', 'grok']);
  });
});

describe('groupCanExpand', () => {
  it('blocks a cursor-only empty workspace and allows mixed groups', () => {
    const cursorEmpty = toSingleton(
      project({
        id: 'cursor:proj:ws',
        agentId: 'cursor',
        title: 'ws',
        actualPath: 'C:\\Users\\demo\\ws',
        sessionCount: 0,
      }),
    );
    const mixed = groupProjectsByPath(
      [
        project({
          id: 'cursor:proj:app',
          agentId: 'cursor',
          title: 'app',
          actualPath: 'C:\\Users\\demo\\app',
          sessionCount: 0,
        }),
        project({
          id: 'claude:proj:app',
          agentId: 'claude',
          title: 'app',
          actualPath: 'C:\\Users\\demo\\app',
          sessionCount: 2,
        }),
      ],
      true,
    )[0];
    expect(groupCanExpand(cursorEmpty)).toBe(false);
    expect(groupCanExpand(mixed)).toBe(true);
  });
});

function toSingleton(row: AgentProject) {
  return groupProjectsByPath([row], false)[0];
}
