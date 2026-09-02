import type { AgentId, RuntimeId } from '@/lib/types';
import type { DoctorDetectResult, DoctorEnvStatus } from './doctor-types';

export interface InstallOutcome {
  ok: boolean;
  action: string;
  logs: string[];
  message: string;
  agent?: DoctorDetectResult | null;
  runtime?: DoctorEnvStatus | null;
  /** Stable machine code for CLI/GUI mapping (`env.not_ready`, `unsupported`, …). */
  code?: string | null;
  /** Structured details (no secrets). */
  details?: unknown;
}

/** Live install/upgrade/uninstall raw UTF-8 output chunk from the desktop event stream. */
export interface InstallProgressPayload {
  agentId?: string | null;
  action?: string;
  /** Canonical field: may be a partial line, empty string, or multi-line block. */
  chunk: string;
  /** @deprecated Prefer {@link InstallProgressPayload.chunk}. */
  line?: string;
}

/** Filter helper: only chunks for this agent (runtime-only chunks have a null agentId). */
export function isProgressForAgent(
  payload: InstallProgressPayload,
  agentId: AgentId,
): boolean {
  if (!payload.agentId) return false;
  return payload.agentId === agentId;
}

export function installProgressChunk(payload: InstallProgressPayload): string {
  if (typeof payload.chunk === 'string') return payload.chunk;
  if (typeof payload.line === 'string') return payload.line;
  return '';
}

/** Mirrors core `catalog::InstallChannelPlan`. */
export interface InstallChannelPlanDto {
  id: string;
  label: string;
  command: string;
  requires: RuntimeId[];
}

/** Mirrors core `catalog::AgentInstallCatalogEntry`. */
export interface AgentInstallCatalogEntryDto {
  agentId: AgentId;
  channels: InstallChannelPlanDto[];
}
