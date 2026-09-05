import { describe, expect, it } from 'vitest';
import type { AgentProject, AgentSession } from '@/lib/types';
import {
  allVisibleSessionsSelected,
  collectSelectableSessions,
  expandedProjectMembers,
  filterVisibleProjects,
  nextSelectedForToggleAllVisible,
  clampSessionPage,
  sessionPageCount,
  SESSION_PAGE_SIZE,
  sliceSessionPage,
  toggleSelectedSession,
  visibleSessionsForProject,
} from './projects-list-model';

function project(
  partial: Partial<AgentProject> & Pick<AgentProject, 'id' | 'title'>,
): AgentProject {
  return {
    agentId: 'claude',
    storagePath: `C:\\Users\\demo\\.claude\\projects\\${partial.id}`,
    relativePath: `projects/${partial.id}`,
    sessionCount: 1,
    sizeBytes: 192_000,
    updatedAt: '2026-08-18T12:00:00.000Z',
    actualPath: `C:\\Users\\demo\\${partial.title}`,
    preview: null,
    ...partial,
  };
}

function session(
  partial: Partial<AgentSession> & Pick<AgentSession, 'id' | 'projectId' | 'title'>,
): AgentSession {
  return {
    agentId: 'claude',
    path: `C:\\Users\\demo\\.claude\\projects\\${partial.projectId}\\${partial.id}.jsonl`,
    relativePath: `projects/${partial.projectId}/${partial.id}.jsonl`,
    sizeBytes: 64_000,
    updatedAt: '2026-08-18T12:00:00.000Z',
    preview: null,
    cwd: `C:\\Users\\demo\\${partial.projectId}`,
    sessionId: partial.id,
    ...partial,
  };
}

const app = project({
  id: 'claude:proj:app',
  title: 'app',
  alias: '工作区',
  preview: 'workspace notes',
  actualPath: 'C:\\Users\\demo\\app',
});
const perf = project({
  id: 'claude:proj:perf',
  title: 'perf',
  preview: 'profiling notes',
  actualPath: 'C:\\Users\\demo\\perf',
});
const docs = project({
  id: 'claude:proj:docs',
  title: 'docs',
  preview: 'readme polish',
  actualPath: 'C:\\Users\\demo\\docs',
});

const appToken = session({
  id: 'sess-a1',
  projectId: app.id,
  title: '修复登录页 token 过期问题',
  preview: '需要检查 refresh 流程',
});
const appTests = session({
  id: 'sess-a2',
  projectId: app.id,
  title: '补充单元测试覆盖率',
  preview: '给 auth 模块补测试',
});
const perfCold = session({
  id: 'sess-p1',
  projectId: perf.id,
  title: '冷启动耗时',
  preview: '分析启动路径',
});
const docsReadme = session({
  id: 'sess-d1',
  projectId: docs.id,
  title: '中文文档润色',
  preview: '请润色 README 中的安装说明',
});

const projects = [app, perf, docs];
const sessionsByProject: Record<string, AgentSession[]> = {
  [app.id]: [appToken, appTests],
  [perf.id]: [perfCold],
  [docs.id]: [docsReadme],
};

function kidsFor(
  projectId: string,
  q: string,
  map: Record<string, AgentSession[]> = sessionsByProject,
): AgentSession[] {
  return visibleSessionsForProject(projectId, projects, q, map);
}

describe('filterVisibleProjects', () => {
  it('shows all projects when the search is empty', () => {
    expect(filterVisibleProjects(projects, '', {}).map((p) => p.id)).toEqual([
      app.id,
      perf.id,
      docs.id,
    ]);
    expect(filterVisibleProjects(projects, '', sessionsByProject).map((p) => p.id)).toEqual([
      app.id,
      perf.id,
      docs.id,
    ]);
  });

  it('keeps a parent when the project itself matches', () => {
    expect(filterVisibleProjects(projects, 'app', sessionsByProject).map((p) => p.id)).toEqual([
      app.id,
    ]);
  });

  it('keeps a parent when a loaded child session matches', () => {
    expect(filterVisibleProjects(projects, 'token', sessionsByProject).map((p) => p.id)).toEqual([
      app.id,
    ]);
  });

  it('drops a parent when neither the project nor loaded kids match', () => {
    expect(filterVisibleProjects(projects, 'xyz', sessionsByProject)).toEqual([]);
  });

  it('drops a parent when the project does not match and kids are not loaded', () => {
    expect(filterVisibleProjects(projects, 'token', {}).map((p) => p.id)).toEqual([]);
    expect(
      filterVisibleProjects(projects, 'token', { [perf.id]: [perfCold] }).map((p) => p.id),
    ).toEqual([]);
  });
});

describe('visibleSessionsForProject', () => {
  it('returns every loaded kid when search is empty', () => {
    expect(visibleSessionsForProject(app.id, projects, '', sessionsByProject).map((s) => s.id)).toEqual([
      appToken.id,
      appTests.id,
    ]);
  });

  it('returns all kids when the project itself matches', () => {
    expect(
      visibleSessionsForProject(app.id, projects, 'app', sessionsByProject).map((s) => s.id),
    ).toEqual([appToken.id, appTests.id]);
  });

  it('returns only matching kids when the project does not match', () => {
    expect(
      visibleSessionsForProject(app.id, projects, 'token', sessionsByProject).map((s) => s.id),
    ).toEqual([appToken.id]);
  });

  it('keeps the parent session when a Cursor subagent matches', () => {
    const parent = session({
      id: 'parent',
      projectId: app.id,
      title: '主会话',
      sessionId: '0e435bc1-cf05-4a9a-b036-8902f810bd86',
      relativePath:
        'projects/ws/agent-transcripts/0e435bc1-cf05-4a9a-b036-8902f810bd86/0e435bc1-cf05-4a9a-b036-8902f810bd86.jsonl',
    });
    const child = session({
      id: 'child',
      projectId: app.id,
      title: '探查项目状态',
      sessionId: 'deadbeef-0000-0000-0000-000000000001',
      relativePath:
        'projects/ws/agent-transcripts/0e435bc1-cf05-4a9a-b036-8902f810bd86/subagents/deadbeef.jsonl',
    });
    const map = { [app.id]: [parent, child] };
    expect(visibleSessionsForProject(app.id, projects, '探查', map).map((s) => s.id)).toEqual([
      'parent',
      'child',
    ]);
  });

  it('returns an empty list when the project has no loaded kids', () => {
    expect(visibleSessionsForProject(app.id, projects, 'token', {})).toEqual([]);
  });
});

describe('collectSelectableSessions', () => {
  it('includes sessions only from expanded visible projects', () => {
    const q = '';
    const visible = filterVisibleProjects(projects, q, sessionsByProject);
    const expanded = new Set([app.id, docs.id]);
    expect(
      collectSelectableSessions(visible, expanded, (id) => kidsFor(id, q)).map((s) => s.id),
    ).toEqual([appToken.id, appTests.id, docsReadme.id]);
  });

  it('returns nothing when no visible project is expanded', () => {
    const q = 'token';
    const visible = filterVisibleProjects(projects, q, sessionsByProject);
    expect(visible.map((p) => p.id)).toEqual([app.id]);
    expect(
      collectSelectableSessions(visible, new Set(), (id) => kidsFor(id, q)).map((s) => s.id),
    ).toEqual([]);
  });

  it('uses the already-filtered visible kids for an expanded parent', () => {
    const q = 'token';
    const visible = filterVisibleProjects(projects, q, sessionsByProject);
    expect(
      collectSelectableSessions(visible, new Set([app.id]), (id) => kidsFor(id, q)).map((s) => s.id),
    ).toEqual([appToken.id]);
  });
});

describe('expandedProjectMembers', () => {
  it('returns members of expanded groups only', () => {
    const groups = [
      { id: 'app', members: [{ id: 'claude:proj:app' }, { id: 'codex:proj:app' }] },
      { id: 'docs', members: [{ id: 'claude:proj:docs' }] },
    ];
    expect(expandedProjectMembers(groups, new Set()).map((m) => m.id)).toEqual([]);
    expect(expandedProjectMembers(groups, new Set(['app'])).map((m) => m.id)).toEqual([
      'claude:proj:app',
      'codex:proj:app',
    ]);
    expect(
      expandedProjectMembers(groups, new Set(['app', 'docs'])).map((m) => m.id),
    ).toEqual(['claude:proj:app', 'codex:proj:app', 'claude:proj:docs']);
  });
});

describe('session pages', () => {
  it('pages through every row without dropping any', () => {
    const rows = Array.from({ length: 1536 }, (_, i) => i);
    expect(sessionPageCount(rows.length)).toBe(Math.ceil(1536 / SESSION_PAGE_SIZE));
    expect(clampSessionPage(-1, rows.length)).toBe(0);
    expect(clampSessionPage(99, rows.length)).toBe(sessionPageCount(rows.length) - 1);

    const seen = new Set<number>();
    for (let page = 0; page < sessionPageCount(rows.length); page += 1) {
      for (const id of sliceSessionPage(rows, page)) seen.add(id);
    }
    expect(seen.size).toBe(1536);
    expect(sliceSessionPage(rows, 0)).toHaveLength(SESSION_PAGE_SIZE);
    expect(sliceSessionPage(rows, sessionPageCount(rows.length) - 1).at(-1)).toBe(1535);
  });
});

describe('selection', () => {
  const selectable = [appToken, appTests, docsReadme];

  it('toggles one id without mutating the previous set', () => {
    const selected = new Set<string>([appToken.id]);
    const added = toggleSelectedSession(selected, appTests.id);
    expect([...added].sort()).toEqual([appToken.id, appTests.id].sort());
    expect([...selected]).toEqual([appToken.id]);

    const removed = toggleSelectedSession(added, appToken.id);
    expect([...removed]).toEqual([appTests.id]);
    expect([...added].sort()).toEqual([appToken.id, appTests.id].sort());
  });

  it('reports all-visible selected only when every selectable id is selected', () => {
    expect(allVisibleSessionsSelected([], new Set())).toBe(false);
    expect(allVisibleSessionsSelected(selectable, new Set())).toBe(false);
    expect(allVisibleSessionsSelected(selectable, new Set([appToken.id, appTests.id]))).toBe(false);
    expect(
      allVisibleSessionsSelected(
        selectable,
        new Set([appToken.id, appTests.id, docsReadme.id, 'other']),
      ),
    ).toBe(true);
  });

  it('selects all visible ids and keeps unrelated selection', () => {
    const selected = new Set(['stale']);
    const allSelected = allVisibleSessionsSelected(selectable, selected);
    const next = nextSelectedForToggleAllVisible(selected, selectable, allSelected);
    expect(allSelected).toBe(false);
    expect(next.has('stale')).toBe(true);
    expect([...selectable].every((s) => next.has(s.id))).toBe(true);
    expect(selected.has(appToken.id)).toBe(false);
  });

  it('deselects all visible ids and keeps unrelated selection', () => {
    const selected = new Set([appToken.id, appTests.id, docsReadme.id, 'stale']);
    const allSelected = allVisibleSessionsSelected(selectable, selected);
    const next = nextSelectedForToggleAllVisible(selected, selectable, allSelected);
    expect(allSelected).toBe(true);
    expect([...next]).toEqual(['stale']);
    expect(selected.has(appToken.id)).toBe(true);
  });
});
