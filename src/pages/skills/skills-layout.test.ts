import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const file = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  'index.tsx',
);

describe('skills split layout', () => {
  it('keeps the page header and install action in the left split column', () => {
    const source = readFileSync(file, 'utf8');
    const splitStart = source.indexOf('ref={splitRef}');
    const headerStart = source.indexOf('pageRhythm.workbenchHeader', splitStart);
    const installStart = source.indexOf("setInstallOpen(true)", headerStart);
    const previewStart = source.indexOf('{previewShellMounted && previewTarget ?', splitStart);

    expect(splitStart).toBeGreaterThanOrEqual(0);
    expect(headerStart).toBeGreaterThan(splitStart);
    expect(installStart).toBeGreaterThan(headerStart);
    expect(installStart).toBeLessThan(previewStart);
    expect(previewStart).toBeGreaterThan(headerStart);
    expect(source).toContain('className="flex min-h-0 min-w-0 flex-1 flex-col"');
  });
});
