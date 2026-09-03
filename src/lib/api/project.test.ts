import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { resetBackend, setBackend } from '@/app/runtime';
import { resetProjectMock } from '@/dev/mocks/project';
import { createBackend as createMockBackend } from '@/dev/mocks/create-backend';
import { createBackend as createTauriBackend } from '@/lib/backend/tauri/create-backend';
import {
  deleteAgentSession,
  deleteAgentSessions,
  getAgentProjectExcerpts,
  getProjectMetadata,
  listAgentProjects,
  listAgentProjectSessions,
  mapAgentProject,
  mapAgentProjectExcerpt,
  mapAgentSession,
  setShowHiddenProjects,
  upsertProjectMeta,
} from '@/lib/api/project';

const invokeMock = vi.fn();

vi.mock('@tauri-apps/api/core', () => ({
  isTauri: () => true,
  invoke: (...args: unknown[]) => invokeMock(...args),
  Channel: class {
    onmessage: ((ev: unknown) => void) | null = null;
  },
}));

describe('project API (browser mock)', () => {
  beforeEach(() => {
    resetBackend();
    setBackend(createMockBackend());
    resetProjectMock();
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
    resetProjectMock();
    resetBackend();
  });

  it('map helpers are identity copies', () => {
    const p = {
      id: 'claude:proj:-C-Users-x',
      agentId: 'claude' as const,
      title: 't',
      storagePath: 'p',
      actualPath: null,
      relativePath: 'projects/-C-Users-x',
      sessionCount: 1,
      sizeBytes: 1,
      updatedAt: 't0',
      preview: null,
      messageCount: null,
      hidden: false,
    };
    expect(mapAgentProject(p)).toEqual(p);
    expect(mapAgentProject(p)).not.toBe(p);

    const s = {
      id: 'claude:projects/-C-Users-x/a.jsonl',
      projectId: p.id,
      agentId: 'claude' as const,
      title: 't',
      cwd: null,
      path: 'p',
      relativePath: 'projects/-C-Users-x/a.jsonl',
      sizeBytes: 1,
      updatedAt: 't0',
      preview: null,
      messageCount: null,
      sessionId: 'a',
    };
    expect(mapAgentSession(s)).toEqual(s);

    const e = {
      id: s.id,
      agentId: 'claude' as const,
      title: 't',
      cwd: null,
      updatedAt: 't0',
      excerpt: 'body',
    };
    expect(mapAgentProjectExcerpt(e)).toEqual(e);
    expect(mapAgentProjectExcerpt(e)).not.toBe(e);
  });

  it('hide and alias via metadata', async () => {
    const listP = listAgentProjects('claude');
    await vi.runAllTimersAsync();
    const projects = await listP;
    const id = projects[0].id;

    const hideP = upsertProjectMeta(id, { hidden: true, alias: 'Login App' });
    await vi.runAllTimersAsync();
    await hideP;

    const hiddenListP = listAgentProjects('claude', false);
    await vi.runAllTimersAsync();
    expect(await hiddenListP).toEqual([]);

    const shownP = listAgentProjects('claude', true);
    await vi.runAllTimersAsync();
    const shown = await shownP;
    expect(shown[0].hidden).toBe(true);
    expect(shown[0].alias).toBe('Login App');

    const showP = setShowHiddenProjects(true);
    await vi.runAllTimersAsync();
    await showP;
    const metaP = getProjectMetadata();
    await vi.runAllTimersAsync();
    expect((await metaP).showHiddenProjects).toBe(true);

    const clearP = upsertProjectMeta(id, { hidden: false, alias: '' });
    await vi.runAllTimersAsync();
    await clearP;
    const restoredP = listAgentProjects('claude', false);
    await vi.runAllTimersAsync();
    const restored = await restoredP;
    expect(restored).toHaveLength(1);
    expect(restored[0].hidden).toBe(false);
    expect(restored[0].alias).toBeFalsy();
  });

  it('cursor project has zero sessions', async () => {
    const listP = listAgentProjects('cursor');
    await vi.runAllTimersAsync();
    const projects = await listP;
    expect(projects.length).toBeGreaterThanOrEqual(1);
    expect(projects[0].sessionCount).toBe(0);
    const sessP = listAgentProjectSessions(projects[0].id);
    await vi.runAllTimersAsync();
    expect(await sessP).toEqual([]);
  });

  it('list filters by agent', async () => {
    const allP = listAgentProjects();
    await vi.runAllTimersAsync();
    const all = await allP;
    expect(all.length).toBeGreaterThanOrEqual(4);

    const claudeP = listAgentProjects('claude');
    await vi.runAllTimersAsync();
    const claude = await claudeP;
    expect(claude.every((p) => p.agentId === 'claude')).toBe(true);
    expect(claude.length).toBe(1);
    expect(claude[0].sessionCount).toBe(2);
  });

  it('list sessions under a project', async () => {
    const listP = listAgentProjects('claude');
    await vi.runAllTimersAsync();
    const projects = await listP;
    const sessP = listAgentProjectSessions(projects[0].id);
    await vi.runAllTimersAsync();
    const sessions = await sessP;
    expect(sessions.length).toBe(2);
    expect(sessions.every((s) => s.projectId === projects[0].id)).toBe(true);
  });

  it('delete one and batch delete sessions', async () => {
    const listP = listAgentProjects('claude');
    await vi.runAllTimersAsync();
    const projects = await listP;
    const projId = projects[0].id;

    const sessP = listAgentProjectSessions(projId);
    await vi.runAllTimersAsync();
    const sessions = await sessP;
    const id = sessions[0].id;

    const delP = deleteAgentSession(id);
    await vi.runAllTimersAsync();
    await delP;

    const sess2P = listAgentProjectSessions(projId);
    await vi.runAllTimersAsync();
    expect((await sess2P).find((p) => p.id === id)).toBeUndefined();

    const remainingP = listAgentProjectSessions(projId);
    await vi.runAllTimersAsync();
    const remaining = await remainingP;
    const ids = remaining.map((p) => p.id);

    const batchP = deleteAgentSessions(ids);
    await vi.runAllTimersAsync();
    expect(await batchP).toBe(ids.length);

    const emptyP = listAgentProjectSessions(projId);
    await vi.runAllTimersAsync();
    expect(await emptyP).toEqual([]);
  });

  it('delete missing throws', async () => {
    const delP = deleteAgentSession('claude:projects/missing.jsonl');
    const assertion = expect(delP).rejects.toThrow(/not found/);
    await vi.runAllTimersAsync();
    await assertion;
  });

  it('excerpts returns mock bodies for session ids', async () => {
    const listP = listAgentProjects('codex');
    await vi.runAllTimersAsync();
    const projects = await listP;
    const sessP = listAgentProjectSessions(projects[0].id);
    await vi.runAllTimersAsync();
    const sessions = await sessP;
    const id = sessions[0].id;

    const exP = getAgentProjectExcerpts([id, 'claude:projects/nope.jsonl']);
    await vi.runAllTimersAsync();
    const excerpts = await exP;
    expect(excerpts).toHaveLength(1);
    expect(excerpts[0].id).toBe(id);
    expect(excerpts[0].excerpt).toContain('---turn:user---');
    expect(excerpts[0].excerpt).toContain('---turn:assistant---');
    expect(excerpts[0].excerpt).toContain('重构 provider 切换逻辑');
    expect(excerpts[0].excerpt).toContain('工作目录');
  });
});

describe('project API (tauri path)', () => {
  beforeEach(() => {
    invokeMock.mockReset();
    setBackend(createTauriBackend());
  });

  afterEach(() => {
    resetBackend();
  });

  it('listAgentProjects invokes list_agent_projects', async () => {
    invokeMock.mockResolvedValueOnce([
      {
        id: 'grok:proj:cwd/C:/work',
        agentId: 'grok',
        title: 'work',
        storagePath: 'p',
        actualPath: null,
        relativePath: 'cwd/C:/work',
        sessionCount: 1,
        sizeBytes: 2,
        updatedAt: 't',
        preview: null,
        messageCount: 1,
      },
    ]);
    const rows = await listAgentProjects('grok');
    expect(invokeMock).toHaveBeenCalledWith('list_agent_projects', {
      agentId: 'grok',
      includeHidden: false,
    });
    expect(rows[0].agentId).toBe('grok');
    expect(rows[0].sessionCount).toBe(1);
  });

  it('metadata APIs invoke corresponding commands', async () => {
    invokeMock.mockResolvedValueOnce({
      version: 1,
      showHiddenProjects: false,
      projects: {},
    });
    const meta = await getProjectMetadata();
    expect(invokeMock).toHaveBeenCalledWith('get_project_metadata', {});
    expect(meta.version).toBe(1);

    invokeMock.mockResolvedValueOnce(undefined);
    await upsertProjectMeta('claude:proj:x', { hidden: true, alias: 'A' });
    expect(invokeMock).toHaveBeenCalledWith('upsert_project_meta', {
      projectId: 'claude:proj:x',
      hidden: true,
      alias: 'A',
    });

    invokeMock.mockResolvedValueOnce(undefined);
    await setShowHiddenProjects(true);
    expect(invokeMock).toHaveBeenCalledWith('set_show_hidden_projects', { show: true });
  });

  it('listAgentProjects passes includeHidden', async () => {
    invokeMock.mockResolvedValueOnce([]);
    await listAgentProjects('claude', true);
    expect(invokeMock).toHaveBeenCalledWith('list_agent_projects', {
      agentId: 'claude',
      includeHidden: true,
    });
  });

  it('listAgentProjectSessions invokes list_agent_project_sessions', async () => {
    invokeMock.mockResolvedValueOnce([]);
    await listAgentProjectSessions('claude:proj:-C-Users-x');
    expect(invokeMock).toHaveBeenCalledWith('list_agent_project_sessions', {
      projectId: 'claude:proj:-C-Users-x',
    });
  });

  it('delete / batch / excerpts invoke corresponding commands', async () => {
    invokeMock.mockResolvedValueOnce(undefined);
    await deleteAgentSession('id-1');
    expect(invokeMock).toHaveBeenCalledWith('delete_agent_session', { id: 'id-1' });

    invokeMock.mockResolvedValueOnce(2);
    await expect(deleteAgentSessions(['a', 'b'])).resolves.toBe(2);
    expect(invokeMock).toHaveBeenCalledWith('delete_agent_sessions', { ids: ['a', 'b'] });

    invokeMock.mockResolvedValueOnce([
      {
        id: 'a',
        agentId: 'claude',
        title: 't',
        cwd: null,
        updatedAt: 't',
        excerpt: 'x',
      },
    ]);
    const ex = await getAgentProjectExcerpts(['a']);
    expect(invokeMock).toHaveBeenCalledWith('get_agent_project_excerpts', { ids: ['a'] });
    expect(ex[0].excerpt).toBe('x');
  });
});
