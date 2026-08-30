import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const src = readFileSync(path.join(path.dirname(fileURLToPath(import.meta.url)), 'ConfigEditor.tsx'), 'utf8');

describe('ConfigEditor', () => {
  it('delegates JSON/TOML chrome to SourcePreview without pretty-print while editing', () => {
    expect(src).toContain('SourcePreview');
    expect(src).toContain('pretty={false}');
    expect(src).toContain('density="editor"');
  });
});
