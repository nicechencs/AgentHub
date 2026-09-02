import type {
  AgentId,
  AgentProject,
  AgentProjectExcerpt,
  AgentSession,
  ProjectMetadataFile,
} from '@/lib/types';

export interface ProjectPort {
  listAgentProjects(
    agentId?: AgentId | null,
    includeHidden?: boolean,
  ): Promise<AgentProject[]>;
  listAgentProjectSessions(projectId: string): Promise<AgentSession[]>;
  getProjectMetadata(): Promise<ProjectMetadataFile>;
  upsertProjectMeta(
    projectId: string,
    patch: { hidden?: boolean; alias?: string | null },
  ): Promise<void>;
  setShowHiddenProjects(show: boolean): Promise<void>;
  deleteAgentSession(id: string): Promise<void>;
  deleteAgentSessions(ids: string[]): Promise<number>;
  /** @deprecated Prefer {@link ProjectPort.deleteAgentSession}. */
  deleteAgentProject(id: string): Promise<void>;
  /** @deprecated Prefer {@link ProjectPort.deleteAgentSessions}. */
  deleteAgentProjects(ids: string[]): Promise<number>;
  getAgentProjectExcerpts(ids: string[]): Promise<AgentProjectExcerpt[]>;
}
