import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const dir = path.dirname(fileURLToPath(import.meta.url));

function source(name: string): string {
  return readFileSync(path.join(dir, name), 'utf8');
}

describe('project path hover trigger', () => {
  it('keeps unverified paths on a content-sized trigger, not the flex leftover', () => {
    const tree = source('ProjectTree.tsx');
    const link = source('ProjectPathLink.tsx');

    expect(tree).toContain('<ProjectPathLink path={path} />');
    expect(tree).not.toMatch(/className="min-w-0 flex-1 truncate font-mono text-meta text-muted"/);

    expect(link).toContain('min-w-0 flex-1');
    expect(link).toContain('max-w-full truncate font-mono text-meta text-muted');
    expect(link).not.toMatch(/<Tip[^>]*flex-1/);
  });
});
