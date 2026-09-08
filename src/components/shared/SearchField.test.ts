import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const dir = path.dirname(fileURLToPath(import.meta.url));

describe('SearchField', () => {
  it('keeps the clear control on a 28px target', () => {
    const source = readFileSync(path.join(dir, 'SearchField.tsx'), 'utf8');
    expect(source).toContain('h-7 w-7');
    expect(source).not.toContain('h-5 w-5');
  });
});

describe('Dialog close control', () => {
  it('keeps the corner close control on a 28px target', () => {
    const source = readFileSync(
      path.join(dir, '../ui/dialog.tsx'),
      'utf8',
    );
    expect(source).toContain('h-7 w-7');
    expect(source).not.toContain('p-0.5');
  });
});
