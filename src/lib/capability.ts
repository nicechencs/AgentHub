/** Capability matrix types — aligned with agenthub_core models::capability.
 * Pure model + helpers only. Browser demo matrix lives in src/dev/mocks/capabilities.ts.
 */
import type { TranslateFn } from '@/lib/i18n';

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

/**
 * Provider controls in Connections have two distinct capability boundaries:
 * creating/editing a saved provider and switching it both need the live config
 * writer. Presets are an optional convenience contract and do not prevent a
 * user from entering a custom provider manually. Missing writer capability is
 * intentionally fail-closed.
 */
export interface ProviderCapabilityGate {
  /** Add/edit a Provider/API Key configuration. */
  canManage: boolean;
  /** Apply a saved Provider to the agent's live configuration. */
  canSwitch: boolean;
  /** Whether the agent exposes usable provider presets. */
  canUsePresets: boolean;
  /** Why provider controls are blocked, when blocked. */
  reason?: string;
}

export function providerCapabilityGate(
  capabilities?: AgentCapabilities | null,
  t?: TranslateFn,
): ProviderCapabilityGate {
  const configWrite = capabilities?.configWrite;
  const providerPresets = capabilities?.providerPresets;
  const canSwitch = isCapabilityUsable(configWrite);
  const canUsePresets = isCapabilityUsable(providerPresets);

  if (!canSwitch) {
    return {
      canManage: false,
      canSwitch: false,
      canUsePresets,
      reason:
        configWrite?.reason ??
        (t ? t('connections.capability.configWriteUnsupported') : 'This agent does not support config writes'),
    };
  }
  return { canManage: true, canSwitch: true, canUsePresets };
}
