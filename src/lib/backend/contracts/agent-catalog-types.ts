/**
 * Agent Catalog DTOs — mirror agenthub_core::platform::agent_catalog wire shape.
 */
import type { AgentCapability, Capability } from '@/lib/capability';
import type { AgentKey, RuntimeId } from '@/lib/types';

export interface CatalogCapabilityStateDto {
  level: AgentCapability['level'];
  reason?: string | null;
  minVersion?: string | null;
}

export interface CatalogInstallChannelDto {
  id: string;
  label: string;
  command: string;
  requires: RuntimeId[];
}

/** How a live write occupies this agent's config. Mirrors Core `LiveOccupancy`. */
export type LiveOccupancyDto = 'exclusive' | 'namedSlots' | 'catalogAppend';

export function catalogOccupancy(
  occupancy?: LiveOccupancyDto | null,
): LiveOccupancyDto {
  return occupancy ?? 'exclusive';
}

export function isCatalogAppendOccupancy(
  occupancy?: LiveOccupancyDto | null,
): boolean {
  return occupancy === 'catalogAppend';
}

/** One agent directory row from Core `AgentDescriptor`. */
export interface AgentCatalogEntryDto {
  key: AgentKey;
  displayName: string;
  integrationVersion: number;
  /** Capability id (camelCase) → state */
  capabilities: Record<string, CatalogCapabilityStateDto>;
  installChannels: CatalogInstallChannelDto[];
  configSchemaVersion?: number | null;
  occupancy?: LiveOccupancyDto;
}

export function mapCatalogCapabilities(
  raw: Record<string, CatalogCapabilityStateDto> | undefined,
): Partial<Record<Capability, AgentCapability>> {
  if (!raw) return {};
  const out: Partial<Record<Capability, AgentCapability>> = {};
  for (const [key, val] of Object.entries(raw)) {
    if (!val || typeof val !== 'object' || !('level' in val)) continue;
    out[key as Capability] = {
      level: val.level,
      reason: val.reason ?? undefined,
      minVersion: val.minVersion ?? undefined,
    };
  }
  return out;
}
