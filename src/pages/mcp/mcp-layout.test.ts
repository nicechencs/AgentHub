import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const dir = path.dirname(fileURLToPath(import.meta.url));

function source(name: string): string {
  return readFileSync(path.join(dir, name), 'utf8');
}

describe('mcp layout wiring', () => {
  it('keeps the agent strip mounted while inventory loads so the table does not jump', () => {
    const page = source('index.tsx');
    expect(page).toContain('pageRhythm.chrome');
    expect(page).toContain('<AgentTabStrip');
    expect(page).toContain('TableSkeleton');
    expect(page).not.toContain('ListSkeleton');
    expect(page.indexOf('<AgentTabStrip')).toBeLessThan(page.indexOf('<TableSkeleton'));
    expect(page).toContain('agents={installedAgents}');
  });
});
