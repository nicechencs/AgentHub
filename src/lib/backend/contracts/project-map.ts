import type { AgentProject, AgentProjectExcerpt, AgentSession, ProjectMetadataFile } from '@/lib/types';

export type CoreAgentProject = AgentProject;
export type CoreAgentSession = AgentSession;
export type CoreAgentProjectExcerpt = AgentProjectExcerpt;
export type CoreProjectMetadataFile = ProjectMetadataFile;

export function mapAgentProject(p: CoreAgentProject): AgentProject {
  return { ...p, hidden: !!p.hidden };
}

export function mapAgentSession(s: CoreAgentSession): AgentSession {
  return { ...s };
}

export function mapAgentProjectExcerpt(e: CoreAgentProjectExcerpt): AgentProjectExcerpt {
  return { ...e };
}

export function mapProjectMetadata(m: CoreProjectMetadataFile): ProjectMetadataFile {
  return {
    version: m.version,
    showHiddenProjects: !!m.showHiddenProjects,
    projects: { ...(m.projects ?? {}) },
  };
}
