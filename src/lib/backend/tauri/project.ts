import type { ProjectPort } from '@/lib/backend/contracts';
import {
  mapAgentProject,
  mapAgentProjectExcerpt,
  mapAgentSession,
  mapProjectMetadata,
  type CoreAgentProject,
  type CoreAgentProjectExcerpt,
  type CoreAgentSession,
  type CoreProjectMetadataFile,
} from '@/lib/backend/contracts/project-map';
import { invoke } from './invoke';

export function createTauriProjectPort(): ProjectPort {
  return {
    async listAgentProjects(agentId, includeHidden = false) {
      const rows = await invoke<CoreAgentProject[]>('list_agent_projects', {
        agentId: agentId ?? null,
        includeHidden,
      });
      return rows.map(mapAgentProject);
    },

    async listAgentProjectSessions(projectId) {
      const rows = await invoke<CoreAgentSession[]>('list_agent_project_sessions', {
        projectId,
      });
      return rows.map(mapAgentSession);
    },

    async getProjectMetadata() {
      const doc = await invoke<CoreProjectMetadataFile>('get_project_metadata', {});
      return mapProjectMetadata(doc);
    },

    async upsertProjectMeta(projectId, patch) {
      await invoke('upsert_project_meta', {
        projectId,
        hidden: patch.hidden ?? null,
        alias: patch.alias === undefined ? null : patch.alias,
      });
    },

    async setShowHiddenProjects(show) {
      await invoke('set_show_hidden_projects', { show });
    },

    async deleteAgentSession(id) {
      await invoke('delete_agent_session', { id });
    },

    async deleteAgentSessions(ids) {
      return invoke<number>('delete_agent_sessions', { ids });
    },

    async deleteAgentProject(id) {
      await invoke('delete_agent_session', { id });
    },

    async deleteAgentProjects(ids) {
      return invoke<number>('delete_agent_sessions', { ids });
    },

    async getAgentProjectExcerpts(ids) {
      const rows = await invoke<CoreAgentProjectExcerpt[]>('get_agent_project_excerpts', {
        ids,
      });
      return rows.map(mapAgentProjectExcerpt);
    },
  };
}
