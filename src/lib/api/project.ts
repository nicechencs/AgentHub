/**
 * Project API façade — delegates to app runtime backend.
 */
import { getBackend } from '@/app/runtime';
import type {
  AgentId,
  AgentProject,
  AgentProjectExcerpt,
  AgentSession,
  ProjectMetadataFile,
} from '@/lib/types';

export type {
  CoreAgentProject,
  CoreAgentProjectExcerpt,
  CoreAgentSession,
  CoreProjectMetadataFile,
} from '@/lib/backend/contracts/project-map';
export {
  mapAgentProject,
  mapAgentProjectExcerpt,
  mapAgentSession,
  mapProjectMetadata,
} from '@/lib/backend/contracts/project-map';

export async function listAgentProjects(
  agentId?: AgentId | null,
  includeHidden = false,
): Promise<AgentProject[]> {
  return getBackend().project.listAgentProjects(agentId, includeHidden);
}

export async function listAgentProjectSessions(projectId: string): Promise<AgentSession[]> {
  return getBackend().project.listAgentProjectSessions(projectId);
}

export async function getProjectMetadata(): Promise<ProjectMetadataFile> {
  return getBackend().project.getProjectMetadata();
}

export async function upsertProjectMeta(
  projectId: string,
  patch: { hidden?: boolean; alias?: string | null },
): Promise<void> {
  return getBackend().project.upsertProjectMeta(projectId, patch);
}

export async function setShowHiddenProjects(show: boolean): Promise<void> {
  return getBackend().project.setShowHiddenProjects(show);
}

export async function deleteAgentSession(id: string): Promise<void> {
  return getBackend().project.deleteAgentSession(id);
}

export async function deleteAgentSessions(ids: string[]): Promise<number> {
  return getBackend().project.deleteAgentSessions(ids);
}

/** @deprecated Prefer {@link deleteAgentSession}. */
export async function deleteAgentProject(id: string): Promise<void> {
  return deleteAgentSession(id);
}

/** @deprecated Prefer {@link deleteAgentSessions}. */
export async function deleteAgentProjects(ids: string[]): Promise<number> {
  return deleteAgentSessions(ids);
}

export async function getAgentProjectExcerpts(ids: string[]): Promise<AgentProjectExcerpt[]> {
  return getBackend().project.getAgentProjectExcerpts(ids);
}
