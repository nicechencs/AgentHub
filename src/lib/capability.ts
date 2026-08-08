/** Capability matrix types — aligned with agenthub_core models::capability.
 * Pure model + helpers only. Browser demo matrix lives in src/dev/mocks/capabilities.ts.
 */

export type Capability =
  | 'configWrite'
  | 'accountSwitch'
  | 'apiKeyAccount'
  | 'skills'
  | 'liveBackup'
  | 'structuredStream'
  | 'dangerousMode'
  | 'projectHistory'
  | 'projectDelete'
  | 'providerPresets'
  | 'usage'
  | 'mcp'
  | 'modelSelect'
  | 'sessionResume';

export type CapabilityLevel = 'full' | 'partial' | 'unsupported' | 'planned';

export interface AgentCapability {
  level: CapabilityLevel;
  reason?: string | null;
  minVersion?: string | null;
}

export type AgentCapabilities = Partial<Record<Capability, AgentCapability>>;

export function isCapabilityUsable(cap?: AgentCapability | null): boolean {
  return cap?.level === 'full' || cap?.level === 'partial';
}

export function isCapabilityBlocked(cap?: AgentCapability | null): boolean {
  return !isCapabilityUsable(cap);
}
