import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const src = readFileSync(
  path.join(path.dirname(fileURLToPath(import.meta.url)), 'ConfigFileCard.tsx'),
  'utf8',
);

describe('ConfigFileCard chrome', () => {
  it('uses the shared copyable file name on one path line', () => {
    expect(src).toContain('<CopyableFileName');
    expect(src).not.toMatch(/truncate font-mono text-sm font-medium text-secondary/);
  });
});
