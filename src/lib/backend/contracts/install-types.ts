import type { AgentId, RuntimeId } from '@/lib/types';
import type { DoctorDetectResult, DoctorEnvStatus } from './doctor-types';

export interface InstallOutcome {
  ok: boolean;
  action: string;
  logs: string[];
  message: string;
  agent?: DoctorDetectResult | null;
  runtime?: DoctorEnvStatus | null;
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
