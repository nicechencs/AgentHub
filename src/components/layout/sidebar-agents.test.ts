import { describe, expect, it } from 'vitest';
import { installedCatalogAgents } from './sidebar-agents';

describe('installedCatalogAgents', () => {
  const catalog = [
    { id: 'claude' },
    { id: 'codex' },
    { id: 'dsh' },
    { id: 'kimi' },
  ];

  it('keeps only installed catalog rows in catalog order', () => {
    const installed = installedCatalogAgents(catalog, [
      { agentId: 'dsh', installed: true },
      { agentId: 'claude', installed: true },
      { agentId: 'kimi', installed: false },
    ]);
    expect(installed.map((row) => row.id)).toEqual(['claude', 'dsh']);
  });

  it('picks up an agent installed after the first snapshot', () => {
    const before = installedCatalogAgents(catalog, [
      { agentId: 'claude', installed: true },
    ]);
    expect(before.map((row) => row.id)).toEqual(['claude']);

    const after = installedCatalogAgents(catalog, [
      { agentId: 'claude', installed: true },
      { agentId: 'dsh', installed: true },
    ]);
    expect(after.map((row) => row.id)).toEqual(['claude', 'dsh']);
  });

  it('omits hidden installed agents', () => {
    const installed = installedCatalogAgents(catalog, [
      { agentId: 'claude', installed: true, hidden: true },
      { agentId: 'dsh', installed: true },
    ]);
    expect(installed.map((row) => row.id)).toEqual(['dsh']);
  });
});
