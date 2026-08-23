import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const dir = path.dirname(fileURLToPath(import.meta.url));

function source(name: string): string {
  return readFileSync(path.join(dir, name), 'utf8');
}

describe('settings layout wiring', () => {
  it('keeps the page title on the edge column; only the body uses the reading column', () => {
    const page = source('index.tsx');
    expect(page).toContain('pageRhythm.readingColumn');
    expect(page).not.toContain('pageRhythm.formColumn');
    expect(page.match(/className=\{pageRhythm\.readingColumn\}/g)?.length).toBe(3);
    expect(page).not.toMatch(/readingColumn\}>\s*<PageHeader/);
    expect(page).not.toMatch(/TabsContent value="[^"]+" className=/);
  });
});
