import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const src = readFileSync(path.join(path.dirname(fileURLToPath(import.meta.url)), 'ConfigEditor.tsx'), 'utf8');

describe('ConfigEditor clip chrome', () => {
  it('clips CodeMirror to rounded-card; scroll lives on the inner scroller', () => {
    expect(src).toContain('overflow-hidden rounded-card');
    expect(src).toContain('overflow-auto');
    expect(src).not.toMatch(/className="[^"]*overflow-auto rounded-card/);
  });
});
