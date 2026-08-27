import { describe, expect, it } from 'vitest';
import { MOCK_CAPABILITIES } from '@/dev/mocks/capabilities';
import { createMockConfigPort } from '@/dev/mocks/config';
import { MOCK_INSTALL_CATALOG } from '@/dev/mocks/fixtures/install-catalog';
import { detectHostPlatform } from '@/lib/platform-detect';
import type { Capability } from '@/lib/capability';
import catalog from './catalog-mirror-contract.json';

type ReasonGrid = Record<string, Record<string, string | null>>;
type ReasonMismatch = { agent: string; capability: string };

const CAPABILITIES = catalog.capabilities as Capability[];
const CAPABILITY_LABELS = catalog.capabilityLabels as Record<string, string>;
const SCHEMA_FIELDS = catalog.schemaFields as Record<string, string[] | undefined>;
const CAPABILITY_REASONS = catalog.capabilityReasons as {
  core: ReasonGrid;
  mock: ReasonGrid;
  knownMismatches: ReasonMismatch[];
};

function reasonCell(grid: ReasonGrid, agentId: string, cap: Capability): string | null {
  const row = grid[agentId];
  expect(row, `missing capabilityReasons row for ${agentId}`).toBeTruthy();
  expect(Object.prototype.hasOwnProperty.call(row, cap), `${agentId}.${cap}`).toBe(true);
  const value = row![cap];
  expect(value === null || typeof value === 'string', `${agentId}.${cap}`).toBe(true);
  if (typeof value === 'string') {
    expect(value.trim().length, `${agentId}.${cap}`).toBeGreaterThan(0);
  }
  return value;
}

function mockReason(agentId: string, cap: Capability): string | null {
  return MOCK_CAPABILITIES[agentId]?.[cap]?.reason ?? null;
}

describe('catalog mirror contract', () => {
  it('lists every mock agent and every capability key', () => {
    expect(Object.keys(MOCK_CAPABILITIES).sort()).toEqual([...catalog.agents].sort());
    expect(MOCK_INSTALL_CATALOG.map((row) => row.agentId).sort()).toEqual(
      [...catalog.agents].sort(),
    );
    for (const agentId of catalog.agents) {
      const row = MOCK_CAPABILITIES[agentId];
      expect(row, `missing mock capabilities for ${agentId}`).toBeTruthy();
      expect(Object.keys(row!).sort(), agentId).toEqual([...CAPABILITIES].sort());
    }
  });

  it('covers every capability user-facing label', () => {
    expect(Object.keys(CAPABILITY_LABELS).sort()).toEqual([...CAPABILITIES].sort());
    for (const cap of CAPABILITIES) {
      const label = CAPABILITY_LABELS[cap];
      expect(label, `missing capability label for ${cap}`).toEqual(expect.any(String));
      expect(label.trim().length, cap).toBeGreaterThan(0);
    }
  });

  it('covers every per-cell capability reason and requires mock to match core', () => {
    expect(Object.keys(CAPABILITY_REASONS.core).sort()).toEqual([...catalog.agents].sort());
    expect(Object.keys(CAPABILITY_REASONS.mock).sort()).toEqual([...catalog.agents].sort());
    const mismatched: ReasonMismatch[] = [];
    for (const agentId of catalog.agents) {
      expect(Object.keys(CAPABILITY_REASONS.core[agentId]!).sort(), `core ${agentId}`).toEqual(
        [...CAPABILITIES].sort(),
      );
      expect(Object.keys(CAPABILITY_REASONS.mock[agentId]!).sort(), `mock ${agentId}`).toEqual(
        [...CAPABILITIES].sort(),
      );
      for (const cap of CAPABILITIES) {
        const coreReason = reasonCell(CAPABILITY_REASONS.core, agentId, cap);
        const mockLocked = reasonCell(CAPABILITY_REASONS.mock, agentId, cap);
        expect(mockReason(agentId, cap), `${agentId}.${cap}`).toEqual(mockLocked);
        if (coreReason !== mockLocked) {
          mismatched.push({ agent: agentId, capability: cap });
        }
      }
    }
    expect(CAPABILITY_REASONS.knownMismatches).toEqual([]);
    expect(mismatched).toEqual([]);
  });

  it('covers production config schema field names served by mock', async () => {
    const port = createMockConfigPort();
    const schemaAgents = Object.keys(SCHEMA_FIELDS).sort();
    expect(schemaAgents.every((agentId) => catalog.agents.includes(agentId))).toBe(true);
    for (const agentId of catalog.agents) {
      const expected = SCHEMA_FIELDS[agentId];
      if (!expected) {
        await expect(port.getAgentConfigSchema(agentId)).rejects.toThrow(
          `unsupported config projector for ${agentId}`,
        );
        continue;
      }
      expect(expected.length, agentId).toBeGreaterThan(0);
      const schema = await port.getAgentConfigSchema(agentId);
      expect(
        schema.fields.map((field) => field.key),
        agentId,
      ).toEqual(expected);
    }
  });

  it('mirrors install channel ids for the current host', () => {
    const platform = detectHostPlatform() === 'windows' ? 'windows' : 'unix';
    const expected = catalog.channels[platform];
    for (const agentId of catalog.agents) {
      const row = MOCK_INSTALL_CATALOG.find((entry) => entry.agentId === agentId);
      expect(row, `missing mock install catalog for ${agentId}`).toBeTruthy();
      expect(
        row!.channels.map((channel) => channel.id),
        agentId,
      ).toEqual(expected[agentId as keyof typeof expected]);
    }
  });
});
