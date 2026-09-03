import type { AgentKey, RuntimeId } from '@/lib/types';
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
  /** May be a partial line, empty string, or multi-line block. */
  chunk: string;
}

/** Filter helper: only chunks for this agent (runtime-only chunks have a null agentId). */
export function isProgressForAgent(
  payload: InstallProgressPayload,
  agentId: AgentKey,
): boolean {
  if (!payload.agentId) return false;
  return payload.agentId === agentId;
}

export function installProgressChunk(payload: InstallProgressPayload): string {
  return typeof payload.chunk === 'string' ? payload.chunk : '';
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
  agentId: AgentKey;
  channels: InstallChannelPlanDto[];
}
