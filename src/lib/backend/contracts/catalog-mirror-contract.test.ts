import { describe, expect, it } from 'vitest';
import { MOCK_CAPABILITIES } from '@/dev/mocks/capabilities';
import { MOCK_INSTALL_CATALOG } from '@/dev/mocks/fixtures/install-catalog';
import { detectHostPlatform } from '@/lib/platform-detect';
import type { Capability } from '@/lib/capability';
import catalog from './catalog-mirror-contract.json';

const CAPABILITIES = catalog.capabilities as Capability[];

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
