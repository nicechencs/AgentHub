import type { ProjectPort } from '@/lib/backend/contracts';
import { delay } from '@/dev/mocks/delay';
import type { AgentProject, AgentSession, ProjectMetadataFile, ProjectUserMeta } from '@/lib/types';
import { seedMockProjects, seedMockSessions } from './fixtures/projects';

let mockProjects: AgentProject[] = seedMockProjects();
let mockSessions: AgentSession[] = seedMockSessions();
let mockMeta: ProjectMetadataFile = {
  version: 1,
  showHiddenProjects: false,
  projects: {},
};

export function resetProjectMock() {
  mockProjects = seedMockProjects();
  mockSessions = seedMockSessions();
  mockMeta = { version: 1, showHiddenProjects: false, projects: {} };
}

function mockExcerpt(p: AgentSession): string {
  const topic = p.preview?.trim() || p.title;
  const cwd = p.cwd ?? '未知';
  return [
    topic,
    `工作目录：${cwd}`,
    `已按这条会话继续，下一步建议先核对现有实现再改。`,
  ].join('\n---\n');
}

function applyMeta(rows: AgentProject[]): AgentProject[] {
  return rows.map((p) => {
    const m = mockMeta.projects[p.id];
    if (!m) return { ...p, hidden: false, alias: p.alias ?? null };
    return {
      ...p,
      hidden: !!m.hidden,
      alias: m.alias ?? null,
    };
  });
}

export function createMockProjectPort(): ProjectPort {
  return {
    async listAgentProjects(agentId, includeHidden = false) {
      await delay(120);
      let rows = !agentId
        ? mockProjects.map((p) => ({ ...p }))
        : mockProjects.filter((p) => p.agentId === agentId).map((p) => ({ ...p }));
      rows = applyMeta(rows);
      if (!includeHidden) rows = rows.filter((p) => !p.hidden);
      return rows;
    },

    async listAgentProjectSessions(projectId) {
      await delay(80);
      return mockSessions.filter((s) => s.projectId === projectId).map((s) => ({ ...s }));
    },

    async getProjectMetadata() {
      await delay(40);
      return {
        version: mockMeta.version,
        showHiddenProjects: mockMeta.showHiddenProjects,
        projects: { ...mockMeta.projects },
      };
    },

    async upsertProjectMeta(projectId, patch) {
      await delay(60);
      const cur: ProjectUserMeta = { ...(mockMeta.projects[projectId] ?? {}) };
      if (patch.hidden !== undefined) cur.hidden = patch.hidden;
      if (patch.alias !== undefined) {
        const t = (patch.alias ?? '').trim();
        cur.alias = t ? t : null;
      }
      const empty = !cur.hidden && !cur.alias;
      if (empty) delete mockMeta.projects[projectId];
      else mockMeta.projects[projectId] = cur;
    },

    async setShowHiddenProjects(show) {
      await delay(40);
      mockMeta.showHiddenProjects = show;
    },

    async deleteAgentProject(id) {
      await delay(80);
      const i = mockSessions.findIndex((p) => p.id === id);
      if (i < 0) throw new Error(`project not found: ${id}`);
      const removed = mockSessions[i];
      mockSessions.splice(i, 1);
      const proj = mockProjects.find((p) => p.id === removed.projectId);
      if (proj) {
        proj.sessionCount = Math.max(0, proj.sessionCount - 1);
        proj.sizeBytes = Math.max(0, proj.sizeBytes - removed.sizeBytes);
        if (proj.sessionCount === 0 && proj.agentId !== 'cursor') {
          mockProjects = mockProjects.filter((p) => p.id !== proj.id);
        }
      }
    },

    async deleteAgentProjects(ids) {
      await delay(100);
      let n = 0;
      for (const id of ids) {
        const i = mockSessions.findIndex((p) => p.id === id);
        if (i < 0) continue;
        const removed = mockSessions[i];
        mockSessions.splice(i, 1);
        n += 1;
        const proj = mockProjects.find((p) => p.id === removed.projectId);
        if (proj) {
          proj.sessionCount = Math.max(0, proj.sessionCount - 1);
          proj.sizeBytes = Math.max(0, proj.sizeBytes - removed.sizeBytes);
          if (proj.sessionCount === 0 && proj.agentId !== 'cursor') {
            mockProjects = mockProjects.filter((p) => p.id !== proj.id);
          }
        }
      }
      return n;
    },

    async getAgentProjectExcerpts(ids) {
      await delay(100);
      return mockSessions
        .filter((p) => ids.includes(p.id))
        .map((p) => ({
          id: p.id,
          agentId: p.agentId,
          title: p.title,
          cwd: p.cwd,
          updatedAt: p.updatedAt,
          excerpt: mockExcerpt(p),
        }));
    },
  };
}
